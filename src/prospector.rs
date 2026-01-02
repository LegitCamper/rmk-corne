use crate::Irqs;
use defmt::info;
use embassy_futures::yield_now;
use embassy_nrf::{
    Peri,
    gpio::{Level, Output, OutputDrive},
    peripherals::{P0_29, P1_11, P1_12, P1_13, P1_14, P1_15, SPI3},
    spim::{self, Frequency, Spim},
};
use embassy_time::{Instant, Timer};
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{Rgb565, raw::RawU16},
    prelude::{Point, RgbColor, Size},
    primitives::Rectangle,
};
use st7789v2_driver::{HORIZONTAL, ST7789V2};

slint::include_modules!();

const WIDTH: usize = 280;
const HEIGHT: usize = 240;

type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

fn create_slint_app() -> MainWindow {
    let ui = MainWindow::new().expect("Failed to load UI");

    ui
}

pub async fn run(mut display: DISPLAY) {
    info!("Starting display");

    let window = slint::platform::software_renderer::MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
    );
    window.set_size(slint::PhysicalSize::new(WIDTH as u32, HEIGHT as u32));
    slint::platform::set_platform(alloc::boxed::Box::new(MyPlatform {
        window: window.clone(),
        instant: Instant::now(),
    }))
    .unwrap();

    let _ui = create_slint_app();

    let mut line = [slint::platform::software_renderer::Rgb565Pixel(150); WIDTH];
    loop {
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(DisplayWrapper {
                display: &mut display,
                line_buffer: &mut line,
            });
        });

        // if window.has_active_animations() {
        //     continue;
        // }

        yield_now().await;
    }
}

pub async fn create_display(pins: ProspectorPins) -> (DISPLAY, Output<'static>) {
    let mut config = spim::Config::default();
    config.frequency = Frequency::M1;
    let spim = Spim::new_txonly(pins.spi, Irqs, pins.sck, pins.mosi, config);

    let mut bl = Output::new(pins.bl, Level::Low, OutputDrive::Standard);
    let dc = Output::new(pins.dc, Level::Low, OutputDrive::Standard);
    let cs = Output::new(pins.cs, Level::High, OutputDrive::Standard);
    let rst = Output::new(pins.rst, Level::Low, OutputDrive::Standard);

    let mut display = ST7789V2::new(
        spim,
        dc,
        cs,
        rst,
        false,
        HORIZONTAL,
        WIDTH as u32,
        HEIGHT as u32,
    );

    display.init(&mut embassy_time::Delay).unwrap();

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

struct MyPlatform {
    window: alloc::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
    instant: Instant,
}

impl slint::platform::Platform for MyPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<alloc::rc::Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_micros(
            Instant::now()
                .checked_duration_since(self.instant)
                .unwrap()
                .as_micros(),
        )
    }
}

struct DisplayWrapper<'a, T> {
    display: &'a mut T,
    line_buffer: &'a mut [slint::platform::software_renderer::Rgb565Pixel],
}
impl<T: DrawTarget<Color = Rgb565>> slint::platform::software_renderer::LineBufferProvider
    for DisplayWrapper<'_, T>
{
    type TargetPixel = slint::platform::software_renderer::Rgb565Pixel;
    fn process_line(
        &mut self,
        line: usize,
        range: core::ops::Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        // Render into the line
        render_fn(&mut self.line_buffer[range.clone()]);

        // Send the line to the screen using DrawTarget::fill_contiguous
        self.display
            .fill_contiguous(
                &Rectangle::new(
                    Point::new(range.start as _, line as _),
                    Size::new(range.len() as _, 1),
                ),
                self.line_buffer[range.clone()]
                    .iter()
                    .map(|p| RawU16::new(p.0).into()),
            )
            .map_err(drop)
            .unwrap();
    }
}
