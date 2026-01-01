use crate::Irqs;
use defmt::info;
use embassy_futures::yield_now;
use embassy_nrf::{
    Peri,
    gpio::{Level, Output, OutputDrive},
    peripherals::{P0_29, P1_11, P1_12, P1_13, P1_14, P1_15, SPI3},
    spim::{self, Frequency, Spim},
};
use embassy_time::Timer;
use embedded_graphics::{
    Drawable,
    draw_target::DrawTarget,
    pixelcolor::BinaryColor,
    prelude::{Point, Primitive, RgbColor, Size},
    primitives::{PrimitiveStyle, Rectangle, Triangle},
};
use mousefood::{EmbeddedBackend, EmbeddedBackendConfig, prelude::Rgb565};
use ratatui::{
    Frame, Terminal,
    style::{Style, Stylize},
    widgets::{Block, Paragraph, Wrap},
};
use st7789v2_driver::{ST7789V2, VERTICAL};

const WIDTH: u32 = 280;
const HEIGHT: u32 = 240;

fn draw(frame: &mut Frame) {
    let text = "Ratatui on embedded devices!";
    let paragraph = Paragraph::new(text.dark_gray()).wrap(Wrap { trim: true });
    let bordered_block = Block::bordered()
        .border_style(Style::new().yellow())
        .title("Mousefood");
    frame.render_widget(paragraph.block(bordered_block), frame.area());
}

type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

pub async fn create_display(pins: ProspectorPins) -> DISPLAY {
    let mut config = spim::Config::default();
    config.frequency = Frequency::M1;
    let spim = Spim::new_txonly(pins.spi, Irqs, pins.sck, pins.mosi, config);

    let mut bl = Output::new(pins.bl, Level::Low, OutputDrive::Standard);
    let dc = Output::new(pins.dc, Level::Low, OutputDrive::Standard);
    let cs = Output::new(pins.cs, Level::High, OutputDrive::Standard);
    let rst = Output::new(pins.rst, Level::Low, OutputDrive::Standard);

    let mut display = ST7789V2::new(spim, dc, cs, rst, true, VERTICAL, WIDTH, HEIGHT);

    display.init(&mut embassy_time::Delay).unwrap();

    display.clear(Rgb565::RED).unwrap();
    bl.set_high();
    Timer::after_millis(1000).await;

    display
}

pub async fn run(mut display: DISPLAY) {
    info!("Starting display");
    // let backend = EmbeddedBackend::new(&mut display, EmbeddedBackendConfig::default());
    // let mut terminal = Terminal::new(backend).unwrap();
    //

    // Draw a triangle.
    // Rectangle::new(Point::new(0, 0), Size::new(200, 200))
    //     .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
    //     .draw(&mut display)
    //     .unwrap();

    loop {
        info!("should be red");

        // terminal.draw(draw).unwrap();
        yield_now().await;
    }
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
