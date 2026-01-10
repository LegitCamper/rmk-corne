use alloc::boxed::Box;
use core::cmp;
use defmt::info;
use embassy_futures::yield_now;
use embassy_time::{Instant, Timer};
use mousefood::prelude::*;
use ratatui::{
    Frame, Terminal,
    style::{Style, Stylize},
    widgets::{Block, Paragraph, Wrap},
};

const WIDTH: usize = 280;
const HEIGHT: usize = 240;

fn draw(frame: &mut Frame) {
    let text = "Ratatui on embedded devices!";
    let paragraph = Paragraph::new(text.dark_gray()).wrap(Wrap { trim: true });
    let bordered_block = Block::bordered()
        .border_style(Style::new().yellow())
        .title("Mousefood");
    frame.render_widget(paragraph.block(bordered_block), frame.area());
}

pub async fn run(mut display: display::DISPLAY) {
    info!("Starting display");

    let backend = EmbeddedBackend::new(&mut display, EmbeddedBackendConfig::default());
    let mut terminal = Terminal::new(backend).unwrap();

    loop {
        terminal.draw(draw).unwrap();

        Timer::after_millis(33).await;
    }
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
