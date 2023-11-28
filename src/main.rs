#![no_main]
#![no_std]

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use embedded_hal::blocking::delay::DelayMs;
use fugit::RateExtU32;
use panic_semihosting as _;

use stm32h7xx_hal::{
    delay::DelayExt,
    device::Peripherals,
    gpio::{GpioExt, Speed::VeryHigh},
    pwr::PwrExt,
    rcc::RccExt,
};

use stm32h743::flash::Flash;

#[entry]
fn main() -> ! {
    let (cp, dp) = init();

    let rcc = dp.RCC.constrain();

    let pwr = dp.PWR.constrain();
    let pwrcfg = pwr.vos3().freeze();

    let ccdr = rcc.sys_ck(128_u32.MHz()).freeze(pwrcfg, &dp.SYSCFG);
    let mut delay = cp.SYST.delay(ccdr.clocks);
    hprintln!("Init");
    // GPIO
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);
    let gpiod = dp.GPIOD.split(ccdr.peripheral.GPIOD);
    let gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);

    // LED
    let mut led = gpioc.pc13.into_push_pull_output();

    // SPI Flash
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
    let buf: &[u8; 16] = &[7; 16];
    let address = 0x00;
    flash.write(address, buf).unwrap();
    let mut rbuf: [u8; 32] = [0; 32];
    // flash.erase_chip().unwrap();
    flash.read(address, &mut rbuf).unwrap();

    let _ = 1;

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
