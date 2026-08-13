//! Autocorrect: buffers the word currently being typed (as resolved output
//! letters, post keymap/tap-hold/layer resolution) and, on a word boundary
//! (space/enter), looks it up against a small dictionary via Levenshtein
//! distance. If a close-enough match is found, it backspaces the mistyped
//! word and retypes the correction.
//!
//! Injection bypasses the normal keymap pipeline entirely -- there's no
//! supported way for an rmk `Processor` to feed synthetic key *actions*
//! back in, so this builds raw `KeyboardReport`s and sends them straight
//! onto `USB_REPORT_CHANNEL` (the same channel rmk's own HID writer task
//! drains), mirroring the pattern rmk itself uses internally for pointing
//! device "caret mode" taps.
//!
//! Known limitations (v1, intentionally scoped down):
//! - Case-insensitive matching only; corrections are always typed lowercase
//!   (an `Action` doesn't carry live modifier state, so there's no reliable
//!   way to know if the mistyped word was capitalized).
//! - Only Space/Enter count as word boundaries.
//! - Central-only: this runs on resolved key actions, which only exist on
//!   the central (peripherals just forward raw matrix events).

use embassy_time::Timer;
use rmk::event::ActionEvent;
use rmk::heapless::Vec as HVec;
use rmk::hid::{KeyboardReport, Report};
use rmk::macros::processor;
use rmk::types::action::Action;
use rmk::types::keycode::{HidKeyCode, KeyCode, from_ascii, to_ascii};

/// Longest word we'll buffer/consider for correction. Longer words just
/// stop being tracked (see `on_action_event`) rather than being buffered.
const MAX_WORD_LEN: usize = 20;

/// Max edit distance for a dictionary word to count as "the correction".
const MAX_DISTANCE: u8 = 2;

/// Starter dictionary of common English (+ a few dev-common) words. Purely
/// additive to extend -- keep entries lowercase ASCII, under MAX_WORD_LEN.
#[rustfmt::skip]
const DICTIONARY: &[&str] = &[
    "the", "and", "that", "have", "for", "not", "with", "you", "this", "but", "his", "from",
    "they", "say", "her", "she", "will", "one", "all", "would", "there", "their", "what", "out",
    "about", "who", "get", "which", "when", "make", "can", "like", "time", "just", "know",
    "take", "people", "into", "year", "your", "good", "some", "could", "them", "see", "other",
    "than", "then", "now", "look", "only", "come", "its", "over", "think", "also", "back",
    "after", "use", "two", "how", "our", "work", "first", "well", "way", "even", "new", "want",
    "because", "any", "these", "give", "day", "most", "should", "very", "through", "just",
    "form", "much", "before", "right", "again", "where", "here", "does", "same", "each", "must",
    "such", "being", "those", "only", "still", "between", "never", "however", "something",
    "though", "really", "sure", "maybe", "always", "already", "probably", "different", "another",
    "example", "because", "people", "before", "little", "world", "school", "important", "every",
    "large", "found", "still", "between", "mean", "keep", "let", "begin", "seem", "help", "talk",
    "turn", "start", "might", "move", "live", "believe", "bring", "happen", "write", "provide",
    "sit", "stand", "lose", "pay", "meet", "include", "continue", "set", "learn", "change",
    "lead", "understand", "watch", "follow", "stop", "create", "speak", "read", "allow", "add",
    "spend", "grow", "open", "walk", "win", "offer", "remember", "love", "consider", "appear",
    "buy", "wait", "serve", "die", "send", "expect", "build", "stay", "fall", "cut", "reach",
    "kill", "remain", "suggest", "raise", "pass", "sell", "require", "report", "decide", "pull",
    "function", "return", "value", "string", "array", "class", "object", "method", "struct",
    "enum", "trait", "module", "import", "export", "const", "static", "public", "private",
    "error", "result", "option", "vector", "buffer", "stream", "socket", "server", "client",
    "request", "response", "config", "default", "process", "thread", "memory", "pointer",
    "reference", "compile", "build", "debug", "release", "commit", "branch", "merge", "rebase",
    "repository", "package", "install", "update", "version", "release", "issue", "comment",
    "review", "approve", "reject", "before", "after", "again", "keyboard", "layout", "layer",
    "modifier", "combo", "macro", "profile", "battery", "firmware", "hardware", "software",
    "definitely", "separate", "occurred", "necessary", "receive", "believe", "achieve", "which",
    "because", "beginning", "business", "calendar", "category", "cemetery", "colleague",
    "committee", "conscience", "definitely", "embarrass", "environment", "existence",
    "experience", "government", "grammar", "harass", "immediately", "independent", "jewelry",
    "judgment", "knowledge", "license", "maintenance", "millennium", "necessary", "occasion",
    "occurred", "parallel", "possession", "privilege", "pronunciation", "recommend", "referred",
    "rhythm", "separate", "successful", "surprise", "tomorrow", "truly", "until", "vacuum",
    "weird",
];

/// Skip correction for anything that looks like a short/no-vowel token
/// (vim motions like "dd", "yy", "dw", "ciw", "gg" are exactly this shape),
/// rather than plausible English prose.
fn looks_like_prose(word: &[u8]) -> bool {
    if word.len() <= 3 {
        return false;
    }
    word.iter()
        .any(|c| matches!(c, b'a' | b'e' | b'i' | b'o' | b'u'))
}

/// Levenshtein distance between `a` and `b`, both assumed `<= MAX_WORD_LEN`
/// (so the result always fits in a `u8`), computed with a two-row DP.
fn levenshtein(a: &[u8], b: &[u8]) -> u8 {
    let mut prev: [u8; MAX_WORD_LEN + 1] = [0; MAX_WORD_LEN + 1];
    let mut cur: [u8; MAX_WORD_LEN + 1] = [0; MAX_WORD_LEN + 1];

    for (j, slot) in prev.iter_mut().enumerate().take(b.len() + 1) {
        *slot = j as u8;
    }

    for (i, &ca) in a.iter().enumerate() {
        cur[0] = (i + 1) as u8;
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev[..=b.len()].copy_from_slice(&cur[..=b.len()]);
    }

    prev[b.len()]
}

/// Find the closest dictionary word within `MAX_DISTANCE`, if any. Returns
/// `None` if `word` is already an exact dictionary entry (nothing to fix)
/// or if no candidate is close enough.
fn find_correction(word: &[u8]) -> Option<&'static str> {
    let mut best: Option<(&'static str, u8)> = None;
    for &candidate in DICTIONARY {
        let cand_bytes = candidate.as_bytes();
        if cand_bytes == word {
            // Already correct as typed.
            return None;
        }
        if cand_bytes.len() > MAX_WORD_LEN {
            continue;
        }
        let dist = levenshtein(word, cand_bytes);
        if dist <= MAX_DISTANCE && best.is_none_or(|(_, best_dist)| dist < best_dist) {
            best = Some((candidate, dist));
        }
    }
    best.map(|(word, _)| word)
}

/// Send one press+release `KeyboardReport` for a plain (unmodified) key.
async fn tap(keycode: HidKeyCode) {
    rmk::channel::USB_REPORT_CHANNEL
        .send(Report::KeyboardReport(KeyboardReport {
            modifier: 0,
            reserved: 0,
            leds: 0,
            keycodes: [keycode as u8, 0, 0, 0, 0, 0],
        }))
        .await;
    Timer::after_millis(5).await;
    rmk::channel::USB_REPORT_CHANNEL
        .send(Report::KeyboardReport(KeyboardReport::default()))
        .await;
    Timer::after_millis(5).await;
}

/// Backspace out `backspace_count` characters, retype `corrected`, then
/// retype `boundary` (the space/enter that triggered the check, which was
/// backspaced out along with the mistyped word).
async fn inject_correction(backspace_count: usize, corrected: &str, boundary: HidKeyCode) {
    for _ in 0..backspace_count {
        tap(HidKeyCode::Backspace).await;
    }
    for &byte in corrected.as_bytes() {
        // Dictionary entries are lowercase-only, so `shifted` is always
        // false here -- see the module-level note on capitalization.
        let (keycode, _shifted) = from_ascii(byte);
        tap(keycode).await;
    }
    tap(boundary).await;
}

#[processor(subscribe = [ActionEvent])]
pub struct AutocorrectProcessor {
    buffer: HVec<u8, MAX_WORD_LEN>,
}

impl Default for AutocorrectProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl AutocorrectProcessor {
    pub fn new() -> Self {
        Self {
            buffer: HVec::new(),
        }
    }

    async fn on_action_event(&mut self, event: ActionEvent) {
        // Only react to presses -- releases would double-process every key.
        if !event.keyboard_event.pressed {
            return;
        }

        let hid = match event.action {
            Action::Key(KeyCode::Hid(hid)) => hid,
            _ => {
                // Anything that isn't a plain HID key (layer taps, mouse,
                // media keys, ...) breaks word tracking.
                self.buffer.clear();
                return;
            }
        };

        if hid == HidKeyCode::Backspace {
            self.buffer.pop();
            return;
        }

        if hid == HidKeyCode::Space || hid == HidKeyCode::Enter {
            if looks_like_prose(&self.buffer)
                && let Some(corrected) = find_correction(&self.buffer)
            {
                inject_correction(self.buffer.len(), corrected, hid).await;
            }
            self.buffer.clear();
            return;
        }

        let ascii = to_ascii(hid, false);
        if ascii.is_ascii_lowercase() {
            if self.buffer.push(ascii).is_err() {
                // Overflowed our max tracked length -- give up on this word.
                self.buffer.clear();
            }
        } else {
            // Digit, punctuation, uppercase, etc. -- not a word we track.
            self.buffer.clear();
        }
    }
}
