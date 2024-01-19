#![no_main]
#![no_std]
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use embedded_graphics::primitives::PrimitiveStyle;
use panic_semihosting as _;

use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Alignment, Text, TextStyle};
use embedded_graphics::{image::Image, primitives::Triangle};
use embedded_hal::blocking::delay::DelayMs;
use fugit::RateExtU32;
use st7789::{Orientation, ST7789};
use stm32h743::flash::Flash;
use stm32h7xx_hal::{
    delay::DelayExt,
    device::Peripherals,
    gpio::{GpioExt, Speed::VeryHigh},
    pwr::PwrExt,
    rcc::RccExt,
    spi::{self, NoMiso, SpiExt},
};
use tinybmp::Bmp;

#[entry]
fn main() -> ! {
    let (cp, dp) = init();

    let rcc = dp.RCC.constrain();

    let pwr = dp.PWR.constrain();
    let pwrcfg = pwr.vos1().freeze();

    let ccdr = rcc
        .sys_ck(400_u32.MHz())
        .pclk1(100.MHz())
        .freeze(pwrcfg, &dp.SYSCFG);
    let mut delay = cp.SYST.delay(ccdr.clocks);
    // GPIO
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);
    let gpiod = dp.GPIOD.split(ccdr.peripheral.GPIOD);
    let gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);

    // LED
    let mut led = gpioc.pc13.into_push_pull_output();
    led.set_low();
    // QSPI Flash
    let _qspi_cs = gpiob.pb6.into_alternate::<10>().speed(VeryHigh);

    let sck = gpiob.pb2.into_alternate().speed(VeryHigh);
    let io1 = gpiod.pd12.into_alternate().speed(VeryHigh);
    let io2 = gpioe.pe2.into_alternate().speed(VeryHigh);
    let io3 = gpiod.pd13.into_alternate().speed(VeryHigh);
    let io0 = gpiod.pd11.into_alternate().speed(VeryHigh);

    let mut flash = Flash::new(
        dp.QUADSPI,
        sck,
        io0,
        io1,
        io2,
        io3,
        &ccdr.clocks,
        ccdr.peripheral.QSPI,
    );
    flash.rest_status_registry();

    // SPI DIsplay
    let d_sck = gpioe.pe12.into_alternate();
    let d_mosi = gpioe.pe14.into_alternate();
    let _d_nss = gpioe.pe11.into_alternate::<5>();
    let d_dc = gpioe.pe15.into_push_pull_output();

    let rst = gpioe.pe9.into_push_pull_output();
    let backlight = gpiod.pd15.into_push_pull_output();

    let spi_display_interface = dp.SPI4.spi(
        (d_sck, NoMiso, d_mosi),
        spi::MODE_3,
        3.MHz(),
        ccdr.peripheral.SPI4,
        &ccdr.clocks,
    );
    let display_interface = SPIInterfaceNoCS::new(spi_display_interface, d_dc);
    let mut display = ST7789::new(display_interface, Some(rst), Some(backlight), 240, 320);
    display.init(&mut delay).unwrap();
    display.set_orientation(Orientation::Portrait).unwrap();
    display.clear(Rgb565::BLACK).unwrap();

    let ferris = Bmp::from_slice(include_bytes!("./ferris.bmp")).unwrap();
    let ferris = Image::new(&ferris, Point::new(0, 0));
    ferris.draw(&mut display).unwrap();

    // let thin_stroke = PrimitiveStyle::with_stroke(Rgb565::CYAN, 8);

    Text::new(
        "Youmu",
        Point { x: 120, y: 250 },
        MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_LIGHT_CYAN),
    )
    .draw(&mut display)
    .unwrap();
    let thin_stroke = PrimitiveStyle::with_stroke(Rgb565::CSS_LIGHT_CYAN, 8);
    // let thick_stroke = PrimitiveStyle::with_stroke(Rgb565::CSS_LIGHT_CYAN, 3);

    Triangle::new(
        Point::new(0, 310),
        Point::new(120, 310),
        Point::new(240, 250),
    )
    .into_styled(thin_stroke)
    .draw(&mut display)
    .unwrap();

    Triangle::new(
        Point::new(240, 310),
        Point::new(120, 310),
        Point::new(0, 250),
    )
    .into_styled(thin_stroke)
    .draw(&mut display)
    .unwrap();

    loop {
        led.toggle();
        delay.delay_ms(200_u16);
    }
}

fn init() -> (cortex_m::Peripherals, Peripherals) {
    let cp = cortex_m::Peripherals::take().unwrap();
    let dp = Peripherals::take().unwrap();
    (cp, dp)
}
