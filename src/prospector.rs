use defmt::info;
use embedded_graphics::mono_font::ascii;
use embedded_graphics::pixelcolor::Rgb565;
use kolibri_embedded_gui::label::Label;
use kolibri_embedded_gui::style::medsize_rgb565_style;
use kolibri_embedded_gui::ui::Ui;
use lcd_async::raw_framebuf::RawFrameBuf;
use rmk::event::ControllerEvent;
use rmk::{channel::CONTROLLER_CHANNEL, types::modifier::ModifierCombination};

use crate::prospector::display::{HEIGHT, WIDTH};

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

pub async fn run(mut fb: RawFrameBuf<Rgb565, &'static mut [u8]>, mut display: display::DISPLAY) {
    info!("Starting display task");

    let mut rmk_events = CONTROLLER_CHANNEL.subscriber().unwrap();
    let mut state = KeyboardState::default();

    let mut needs_redraw = true;

    loop {
        let event_result = embassy_time::with_timeout(
            embassy_time::Duration::from_millis(50),
            rmk_events.next_message_pure(),
        )
        .await;

        match event_result {
            Ok(event) => match event {
                ControllerEvent::SplitPeripheralBattery(half, bat) => {
                    let target = if half == 0 {
                        &mut state.battery_l
                    } else {
                        &mut state.battery_r
                    };
                    if *target != Some(bat) {
                        *target = Some(bat);
                        needs_redraw = true;
                    }
                }
                ControllerEvent::Layer(layer) => {
                    if state.layer != layer {
                        state.layer = layer;
                        needs_redraw = true;
                    }
                }
                ControllerEvent::Modifier(comb) => {
                    if state.modifiers != comb {
                        state.modifiers = comb;
                        needs_redraw = true;
                    }
                }
                _ => {}
            },
            Err(_) => {}
        }

        if needs_redraw {
            let mut ui = Ui::new_fullscreen(&mut fb, medsize_rgb565_style());
            ui.clear_background().unwrap();

            let layer = layer_text(state.layer);
            let batt_l = battery_text("Left", state.battery_l);
            let batt_r = battery_text("Right", state.battery_r);
            let mods = modifiers_text(state.modifiers);

            ui.add(Label::new("Keyboard Status").with_font(ascii::FONT_10X20));
            ui.add(Label::new(layer.as_str()));
            ui.add(Label::new(batt_l.as_str()));
            ui.add(Label::new(batt_r.as_str()));
            ui.add(Label::new(mods.as_str()));

            display
                .show_raw_data(0, 0, WIDTH as u16, HEIGHT as u16, fb.as_mut_bytes())
                .await
                .unwrap();

            needs_redraw = false;
        }
    }
}

pub mod display {
    use crate::Irqs;

    use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
    use embassy_nrf::{
        Peri,
        gpio::{Level, Output, OutputDrive},
        peripherals::{P0_29, P1_11, P1_12, P1_13, P1_14, P1_15, SPI3},
        spim::{self, Frequency, Spim},
    };
    use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
    use embassy_time::{Delay, Timer};
    use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::RgbColor};
    use lcd_async::{
        Builder, Display,
        interface::{self, SpiInterface},
        models::ST7789,
        options::{ColorInversion, Orientation, Rotation},
        raw_framebuf::RawFrameBuf,
    };
    use static_cell::StaticCell;

    const PIXEL_SIZE: usize = 2;
    pub const WIDTH: usize = 280;
    pub const HEIGHT: usize = 240;
    pub const FRAME_SIZE: usize = (WIDTH as usize) * (HEIGHT as usize) * PIXEL_SIZE;

    static FRAME_BUFFER: StaticCell<[u8; FRAME_SIZE]> = StaticCell::new();

    static SPI_BUS: StaticCell<Mutex<NoopRawMutex, Spim<'static>>> = StaticCell::new();

    pub struct ProspectorPins {
        pub spi: Peri<'static, SPI3>,
        pub dc: Peri<'static, P1_12>,
        pub sck: Peri<'static, P1_13>,
        pub cs: Peri<'static, P1_14>,
        pub mosi: Peri<'static, P1_15>,
        pub bl: Peri<'static, P1_11>,
        pub rst: Peri<'static, P0_29>,
    }

    pub type DISPLAY = Display<
        SpiInterface<
            SpiDevice<'static, NoopRawMutex, Spim<'static>, Output<'static>>,
            Output<'static>,
        >,
        ST7789,
        Output<'static>,
    >;

    pub async fn create_display(
        pins: ProspectorPins,
    ) -> (
        RawFrameBuf<Rgb565, &'static mut [u8]>,
        DISPLAY,
        Output<'static>,
    ) {
        let mut config = spim::Config::default();
        config.frequency = Frequency::M32;
        let spim = Spim::new_txonly(pins.spi, Irqs, pins.sck, pins.mosi, config.clone());

        let mut bl = Output::new(pins.bl, Level::Low, OutputDrive::Standard);
        let dc = Output::new(pins.dc, Level::Low, OutputDrive::Standard);
        let cs = Output::new(pins.cs, Level::High, OutputDrive::Standard);
        let rst = Output::new(pins.rst, Level::Low, OutputDrive::Standard);

        let spi_bus = SPI_BUS.init(Mutex::new(spim));
        let spi_dev = SpiDevice::new(spi_bus, cs);
        let di = interface::SpiInterface::new(spi_dev, dc);

        let mut display = Builder::new(ST7789, di)
            .reset_pin(rst)
            .display_size(WIDTH as u16, HEIGHT as u16)
            .orientation(Orientation {
                rotation: Rotation::Deg0,
                mirrored: false,
            })
            .display_offset(0, 0)
            .invert_colors(ColorInversion::Inverted)
            .init(&mut Delay)
            .await
            .unwrap();

        let frame_buffer = FRAME_BUFFER.init_with(|| [0; FRAME_SIZE]);

        let mut raw_fb =
            RawFrameBuf::<Rgb565, _>::new(frame_buffer.as_mut_slice(), WIDTH.into(), HEIGHT.into());

        raw_fb.clear(Rgb565::BLACK).unwrap();

        display
            .show_raw_data(0, 0, WIDTH as u16, HEIGHT as u16, raw_fb.as_mut_bytes())
            .await
            .unwrap();

        bl.set_high();
        Timer::after_millis(1000).await;

        (raw_fb, display, bl)
    }
}
