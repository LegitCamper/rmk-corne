use core::cmp;
use defmt::info;
use embassy_futures::yield_now;
use embassy_time::{Instant, Timer};
use slint::{
    PhysicalSize, PlatformError,
    platform::{
        Platform, WindowAdapter, duration_until_next_timer_update, set_platform,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel},
        update_timers_and_animations,
    },
};

slint::include_modules!();

const WIDTH: usize = 280;
const HEIGHT: usize = 240;

pub fn create_slint_app() -> MainWindow {
    let ui = MainWindow::new().expect("Failed to load UI");

    ui
}

pub async fn run(mut display: display::DISPLAY) {
    info!("Starting display");

    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    window.set_size(PhysicalSize::new(WIDTH as u32, HEIGHT as u32));
    set_platform(alloc::boxed::Box::new(MyPlatform {
        window: window.clone(),
        instant: Instant::now(),
    }))
    .unwrap();

    let _ui = create_slint_app();

    let mut line = [Rgb565Pixel(0); WIDTH];
    loop {
        update_timers_and_animations();

        // window.draw_if_needed(|renderer| {
        //     renderer.render_by_line(display::DisplayWrapper {
        //         display: &mut display,
        //         line_buffer: &mut line,
        //     });
        // });

        if !window.has_active_animations() {
            if let Some(duration) = duration_until_next_timer_update() {
                let ms = cmp::min(duration.as_millis(), 1000) as u64; // max 1s
                Timer::after_millis(ms).await;
                continue;
            }
            yield_now().await;
        }

        yield_now().await;
    }
}

struct MyPlatform {
    window: alloc::rc::Rc<MinimalSoftwareWindow>,
    instant: Instant,
}

impl Platform for MyPlatform {
    fn create_window_adapter(&self) -> Result<alloc::rc::Rc<dyn WindowAdapter>, PlatformError> {
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
    use embedded_graphics::{
        draw_target::DrawTarget,
        pixelcolor::{Rgb565, raw::RawU16},
        prelude::{Point, RgbColor, Size},
        primitives::Rectangle,
    };
    use st7789v2_driver::{HORIZONTAL, ST7789V2};

    pub type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

    pub async fn create_display(pins: ProspectorPins) -> (DISPLAY, Output<'static>) {
        let mut config = spim::Config::default();
        config.frequency = Frequency::M32;
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

    pub struct DisplayWrapper<'a, T> {
        pub display: &'a mut T,
        pub line_buffer: &'a mut [slint::platform::software_renderer::Rgb565Pixel],
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
            // render Slint pixels into line buffer slice
            let slice = &mut self.line_buffer[range.start..range.end];
            render_fn(slice);

            // send to ST7789
            let raw_pixels = slice.iter().map(|p| RawU16::new(p.0.swap_bytes()).into());
            let rect = Rectangle::new(
                Point::new(range.start as i32, line as i32),
                Size::new(slice.len() as u32, 1),
            );
            self.display
                .fill_contiguous(&rect, raw_pixels)
                .map_err(drop)
                .unwrap();
        }
    }
}
