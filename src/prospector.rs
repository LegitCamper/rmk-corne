use defmt::info;
use embassy_futures::yield_now;
use embedded_graphics::mono_font::ascii;
use kolibri_embedded_gui::label::Label;
use kolibri_embedded_gui::style::medsize_rgb565_style;
use kolibri_embedded_gui::ui::Ui;
use rmk::event::ControllerEvent;
use rmk::{channel::CONTROLLER_CHANNEL, types::modifier::ModifierCombination};

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

fn battery_text(name: &str, value: Option<u8>) -> heapless::String<32> {
    let mut s = heapless::String::<32>::new();
    match value {
        Some(v) => {
            let _ = core::fmt::write(&mut s, format_args!("{name}: {v}%"));
        }
        None => {
            let _ = core::fmt::write(&mut s, format_args!("{name}: --"));
        }
    }
    s
}

fn layer_text(layer: u8) -> heapless::String<32> {
    let mut s = heapless::String::<32>::new();
    let _ = core::fmt::write(&mut s, format_args!("Layer: {layer}"));
    s
}

fn modifiers_text(mods: ModifierCombination) -> heapless::String<64> {
    let m = normalize_mods(mods);
    let mut s = heapless::String::<64>::new();

    let _ = core::fmt::write(
        &mut s,
        format_args!(
            "Mods: {}{}{}{}",
            if m.ctrl { "CTRL " } else { "" },
            if m.shift { "SHIFT " } else { "" },
            if m.alt { "ALT " } else { "" },
            if m.win { "WIN" } else { "" },
        ),
    );

    if s == "Mods: " {
        let _ = core::fmt::write(&mut s, format_args!("Mods: none"));
    }

    s
}

pub async fn run(mut display: display::DISPLAY) {
    info!("Starting display");

    let mut rmk_events = CONTROLLER_CHANNEL.subscriber().unwrap();
    let mut state = KeyboardState::default();
    let mut changed = true;

    loop {
        let event = rmk_events.next_message_pure().await;

        match event {
            ControllerEvent::SplitPeripheralBattery(half, bat) => {
                if half == 0 {
                    if state.battery_l != Some(bat) {
                        state.battery_l = Some(bat);
                        changed = true;
                    }
                } else if state.battery_r != Some(bat) {
                    state.battery_r = Some(bat);
                    changed = true;
                }
            }
            ControllerEvent::Layer(layer) => {
                if state.layer != layer {
                    state.layer = layer;
                    changed = true;
                }
            }
            ControllerEvent::Modifier(comb) => {
                state.modifiers = comb;
                changed = true;
            }
            _ => {}
        }

        if changed {
            let mut ui = Ui::new_fullscreen(&mut display, medsize_rgb565_style());
            ui.clear_background().unwrap();

            let title = "Keyboard Status";
            let layer = layer_text(state.layer);
            let batt_l = battery_text("Left", state.battery_l);
            let batt_r = battery_text("Right", state.battery_r);
            let mods = modifiers_text(state.modifiers);

            ui.add(Label::new(title).with_font(ascii::FONT_10X20));
            ui.add(Label::new(layer.as_str()));
            ui.add(Label::new(batt_l.as_str()));
            ui.add(Label::new(batt_r.as_str()));
            ui.add(Label::new(mods.as_str()));

            changed = false;
        }

        yield_now().await;
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
}
