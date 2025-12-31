use crate::Irqs;
use embassy_futures::yield_now;
use embassy_nrf::{
    Peri,
    gpio::{Level, Output, OutputDrive},
    peripherals::{P0_29, P1_11, P1_12, P1_13, P1_14, P1_15, SPI3},
    spim::Spim,
};
use mousefood::{EmbeddedBackend, EmbeddedBackendConfig};
use ratatui::{
    Frame, Terminal,
    style::{Style, Stylize},
    widgets::{Block, Paragraph, Wrap},
};
use st7789v2_driver::{HORIZONTAL, ST7789V2};

fn draw(frame: &mut Frame) {
    let text = "Ratatui on embedded devices!";
    let paragraph = Paragraph::new(text.dark_gray()).wrap(Wrap { trim: true });
    let bordered_block = Block::bordered()
        .border_style(Style::new().yellow())
        .title("Mousefood");
    frame.render_widget(paragraph.block(bordered_block), frame.area());
}

type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

pub fn create_display(pins: ProspectorPins) -> DISPLAY {
    let spim = Spim::new_txonly(pins.spi, Irqs, pins.sck, pins.mosi, Default::default());

    let dc = Output::new(pins.dc, Level::Low, OutputDrive::Standard);
    let cs = Output::new(pins.cs, Level::High, OutputDrive::Standard);
    let rst = Output::new(pins.rst, Level::Low, OutputDrive::Standard);

    ST7789V2::new(spim, dc, cs, rst, true, HORIZONTAL, 240, 280)
}

pub async fn run(mut display: DISPLAY) {
    let backend = EmbeddedBackend::new(&mut display, EmbeddedBackendConfig::default());
    let mut terminal = Terminal::new(backend).unwrap();

    loop {
        terminal.draw(draw).unwrap();
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
