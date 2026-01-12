use alloc::format;
use alloc::string::String;
use alloc::vec;
use defmt::info;
use embassy_time::Timer;
use mousefood::prelude::*;
use ratatui::{
    Frame, Terminal,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use rmk::event::ControllerEvent;
use rmk::{channel::CONTROLLER_CHANNEL, types::modifier::ModifierCombination};

use crate::prospector::display::ScaledDisplay;

#[derive(Default, Clone, Copy)]
struct KeyboardState {
    battery_l: Option<u8>,
    battery_r: Option<u8>,
    modifiers: ModifierCombination,
    layer: u8,
}

struct ModifierState {
    win: bool,
    shift: bool,
    ctrl: bool,
    alt: bool,
}

fn normalize_mods(mods: ModifierCombination) -> ModifierState {
    ModifierState {
        win: mods.left_gui() || mods.right_gui(),
        shift: mods.left_shift() || mods.right_shift(),
        ctrl: mods.left_ctrl() || mods.right_ctrl(),
        alt: mods.left_alt() || mods.right_alt(),
    }
}

fn safe_area(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn draw_ui(frame: &mut Frame, state: &KeyboardState) {
    let area = safe_area(frame.area());

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // batteries
            Constraint::Length(1), // modifiers
            Constraint::Min(0),    // layer
        ])
        .split(area);

    let (l_txt, l_style) = battery_text(state.battery_l);
    let (r_txt, r_style) = battery_text(state.battery_r);

    let line = Line::from(vec![
        Span::styled(l_txt, l_style),
        Span::raw("   "),
        Span::styled(r_txt, r_style),
    ]);

    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), rows[0]);

    draw_modifiers(frame, rows[1], state.modifiers);

    let layers = ["Base", "Num", "Nav", "Gaming", "Gaming Upper"];
    let layer = format!("{}", layers[state.layer as usize]);
    let layer_text = vec![Line::from(Span::raw(layer))];
    let layer_para = Paragraph::new(layer_text).alignment(Alignment::Center);
    frame.render_widget(layer_para, rows[2]);
}

fn battery_text(batt: Option<u8>) -> (String, Style) {
    match batt {
        Some(v) => {
            let style = if v < 20 {
                Style::default().fg(Color::Red)
            } else if v < 50 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };

            (format!("{v}%"), style)
        }
        None => (format!("???"), Style::default().fg(Color::DarkGray)),
    }
}

fn draw_modifiers(frame: &mut Frame, area: Rect, mods: ModifierCombination) {
    let mods = normalize_mods(mods);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let items = [
        ("W", mods.win),
        ("S", mods.shift),
        ("C", mods.ctrl),
        ("A", mods.alt),
    ];

    for ((label, active), chunk) in items.into_iter().zip(chunks.iter()) {
        let style = if active {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let para = Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(style);

        frame.render_widget(para, *chunk);
    }
}

pub async fn run(display: display::DISPLAY) {
    info!("Starting display");

    let mut scaled_display = ScaledDisplay::new(display);

    let backend = EmbeddedBackend::new(&mut scaled_display, EmbeddedBackendConfig::default());
    let mut terminal = Terminal::new(backend).unwrap();

    let mut rmk_events = CONTROLLER_CHANNEL.subscriber().unwrap();
    let mut changed = true; // true for first draw

    loop {
        let mut state = KeyboardState::default();

        let event = rmk_events.next_message_pure().await;
        match event {
            ControllerEvent::Layer(layer) => {
                state.layer = layer;
                changed = true;
            }
            ControllerEvent::Modifier(comb) => {
                state.modifiers = comb;
                changed = true;
            }
            _ => {}
        }

        if changed {
            terminal
                .draw(|frame| {
                    draw_ui(frame, &state);
                })
                .unwrap();
            changed = false;

            Timer::after_millis(33).await;
        }
    }
}

pub mod display {
    use crate::Irqs;

    use embassy_nrf::{
        Peri,
        gpio::{Level, Output, OutputDrive},
        peripherals::{P0_29, P1_11, P1_12, P1_13, P1_14, P1_15, SPI3},
        spim::{self, Frequency, Spim},
    };
    use embassy_time::{Delay, Timer};
    use embedded_graphics::{
        Pixel,
        draw_target::DrawTarget,
        prelude::{OriginDimensions, Point, Size},
    };
    use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
    use st7789v2_driver::{HORIZONTAL, ST7789V2};

    const WIDTH: usize = 280;
    const HEIGHT: usize = 240;

    pub type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

    pub struct ProspectorPins {
        pub spi: Peri<'static, SPI3>,
        pub dc: Peri<'static, P1_12>,
        pub sck: Peri<'static, P1_13>,
        pub cs: Peri<'static, P1_14>,
        pub mosi: Peri<'static, P1_15>,
        pub bl: Peri<'static, P1_11>,
        pub rst: Peri<'static, P0_29>,
    }

    pub async fn create_display(pins: ProspectorPins) -> (DISPLAY, Output<'static>) {
        let mut config = spim::Config::default();
        config.frequency = Frequency::M32;
        let spim = Spim::new_txonly(pins.spi, Irqs, pins.sck, pins.mosi, config.clone());

        let mut bl = Output::new(pins.bl, Level::Low, OutputDrive::Standard);
        let dc = Output::new(pins.dc, Level::Low, OutputDrive::Standard);
        let cs = Output::new(pins.cs, Level::High, OutputDrive::Standard);
        let rst = Output::new(pins.rst, Level::Low, OutputDrive::Standard);

        let mut display = ST7789V2::new(
            spim,
            dc,
            cs,
            rst,
            true,
            HORIZONTAL,
            WIDTH as u32,
            HEIGHT as u32,
        );

        display.init(&mut Delay).unwrap();

        display.clear(Rgb565::BLACK).unwrap();
        bl.set_high();
        Timer::after_millis(1000).await;

        (display, bl)
    }

    const SCALE: usize = 4;
    const SCALED_WIDTH: usize = WIDTH / SCALE;
    const SCALED_HEIGHT: usize = HEIGHT / SCALE;

    pub struct ScaledDisplay {
        display: DISPLAY,
    }

    impl ScaledDisplay {
        pub fn new(display: DISPLAY) -> Self {
            Self { display }
        }
    }

    impl DrawTarget for ScaledDisplay {
        type Color = Rgb565;

        type Error = ();

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(coord, color) in pixels {
                let base_x = coord.x * SCALE as i32;
                let base_y = coord.y * SCALE as i32;

                // Draw a 4×4 block for each logical pixel
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let px = base_x as usize + dx;
                        let py = base_y as usize + dy;

                        self.display.draw_iter(core::iter::once(Pixel(
                            Point {
                                x: px as i32,
                                y: py as i32,
                            },
                            color,
                        )))?;
                    }
                }
            }

            Ok(())
        }
    }

    impl OriginDimensions for ScaledDisplay {
        fn size(&self) -> Size {
            Size::new(SCALED_WIDTH as u32, SCALED_HEIGHT as u32)
        }
    }
}
