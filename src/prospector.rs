use alloc::boxed::Box;
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
use st7789v2_driver::FrameBuffer;

slint::include_modules!();

const WIDTH: usize = 280;
const HEIGHT: usize = 240;

pub fn create_slint_app() -> MainWindow {
    let ui = MainWindow::new().expect("Failed to load UI");

    let ui_handle = ui.as_weak();

    ui
}

pub async fn run(mut display: display::DISPLAY, mut fb: FrameBuffer<'static>) {
    info!("Starting display");

    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    window.set_size(PhysicalSize::new(WIDTH as u32, HEIGHT as u32));

    set_platform(Box::new(MyPlatform(window.clone(), Instant::now()))).unwrap();

    let _ui = create_slint_app();

    let mut line = [Rgb565Pixel(0); WIDTH];
    loop {
        update_timers_and_animations();

        let mut did_draw = false;

        // draw ui to framebuffer
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(display::DisplayWrapper {
                framebuffer: &mut fb,
                line_buffer: &mut line,
            });
            did_draw = true;
        });

        if did_draw {
            Timer::after_millis(10).await;
            display.draw_image(fb.get_buffer()).unwrap();
        }

        if !window.has_active_animations() {
            if let Some(duration) = duration_until_next_timer_update() {
                let ms = cmp::min(duration.as_millis(), 1000) as u64; // max 1s
                Timer::after_millis(ms).await;
                continue;
            }
        }

        yield_now().await;
    }
}

struct MyPlatform(alloc::rc::Rc<MinimalSoftwareWindow>, Instant);

impl Platform for MyPlatform {
    fn create_window_adapter(&self) -> Result<alloc::rc::Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_micros(
            Instant::now()
                .checked_duration_since(self.1)
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
        pixelcolor::raw::RawU16,
        prelude::{Point, Size},
        primitives::Rectangle,
    };
    use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
    use st7789v2_driver::{FrameBuffer, HORIZONTAL, ST7789V2};

    pub type DISPLAY = ST7789V2<Spim<'static>, Output<'static>, Output<'static>, Output<'static>>;

    pub async fn create_display(
        pins: ProspectorPins,
    ) -> (DISPLAY, Output<'static>, FrameBuffer<'static>) {
        let config = spim::Config::default();
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

        static mut FRAME_BUFFER: [u8; WIDTH * HEIGHT * 2] = [0; WIDTH * HEIGHT * 2];
        let framebuffer =
            FrameBuffer::new(unsafe { &mut FRAME_BUFFER }, WIDTH as u32, HEIGHT as u32);

        (display, bl, framebuffer)
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

    pub struct DisplayWrapper<'a> {
        pub framebuffer: &'a mut FrameBuffer<'static>,
        pub line_buffer: &'a mut [slint::platform::software_renderer::Rgb565Pixel],
    }

    impl slint::platform::software_renderer::LineBufferProvider for DisplayWrapper<'_> {
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
            self.framebuffer
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
}
