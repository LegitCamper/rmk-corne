use core::fmt::Write;
use defmt::info;
use embassy_futures::{join::join, yield_now};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Watch};
use embassy_time::Timer;
use mousefood::prelude::*;
use ratatui::{
    Frame, Terminal,
    layout::Alignment,
    style::{Style, Stylize},
    widgets::{Block, Paragraph, Wrap},
};
use rmk::{channel::CONTROLLER_CHANNEL, heapless};

const WIDTH: usize = 280;
const HEIGHT: usize = 240;

#[derive(Default, Clone, Copy)]
struct keyboardState {
    battery_l: u16,
    battery_r: u16,
    layer: u8,
    wpm: u16,
}

static STATE_WATCH: Watch<CriticalSectionRawMutex, keyboardState, 2> = Watch::new();

pub async fn run(mut display: display::DISPLAY) {
    info!("Starting display");

    let backend = EmbeddedBackend::new(&mut display, EmbeddedBackendConfig::default());
    let mut terminal = Terminal::new(backend).unwrap();

    let mut start = true;

    join(
        async {
            let mut keyboard_state = STATE_WATCH.receiver().unwrap();
            loop {
                let state = keyboard_state.changed().await;

                terminal
                    .draw(|frame| {
                        let mut s = heapless::String::<64>::new();
                        write!(s, "Layer: {} | WPM: {}", state.layer, state.wpm).unwrap();

                        frame.render_widget(
                            Paragraph::new(s.as_str()).alignment(Alignment::Center),
                            frame.area(),
                        );
                    })
                    .unwrap();
                Timer::after_millis(33).await;
            }
        },
        async {
            let keyboard_state = STATE_WATCH.sender();
            let mut rmk_events = CONTROLLER_CHANNEL.subscriber().unwrap();
            let mut changed = false;

            if start {
                start = false;
                changed = true;
            }

            loop {
                let mut new_state = keyboardState::default();

                let event = rmk_events.next_message_pure().await;
                match event {
                    // rmk::event::ControllerEvent::Battery(_) => todo!(),
                    rmk::event::ControllerEvent::Layer(layer) => {
                        new_state.layer = layer;
                        changed = true;
                    }
                    // rmk::event::ControllerEvent::Modifier(modifier_combination) => todo!(),
                    rmk::event::ControllerEvent::Wpm(wpm) => {
                        new_state.wpm = wpm;
                        changed = true;
                    }
                    _ => {}
                }

                if changed {
                    keyboard_state.send(new_state);

                    Timer::after_millis(100).await;
                }
            }
        },
    )
    .await;
}

pub mod display {
    use super::{HEIGHT, WIDTH};
    use crate::Irqs;

    use embassy_nrf::{
        Peri,
        gpio::{Level, Output, OutputDrive},
        peripherals::{P0_29, P1_11, P1_12, P1_13, P1_14, P1_15, SPI3},
        spim::{self, Frequency, Spim},
    };
    use embassy_time::{Delay, Timer};
    use embedded_graphics::draw_target::DrawTarget;
    use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
    use st7789v2_driver::{HORIZONTAL, ST7789V2};

    pub type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

    pub async fn create_display(pins: ProspectorPins) -> (DISPLAY, Output<'static>) {
        let mut config = spim::Config::default();
        // config.frequency = Frequency::K125;
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

    pub struct ProspectorPins {
        pub spi: Peri<'static, SPI3>,
        pub dc: Peri<'static, P1_12>,
        pub sck: Peri<'static, P1_13>,
        pub cs: Peri<'static, P1_14>,
        pub mosi: Peri<'static, P1_15>,
        pub bl: Peri<'static, P1_11>,
        pub rst: Peri<'static, P0_29>,
    }
}
