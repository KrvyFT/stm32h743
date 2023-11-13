#![no_main]
#![no_std]

use cortex_m_rt::entry;
use embedded_hal::prelude::_embedded_hal_blocking_delay_DelayMs;
use panic_halt as _;

use stm32h7xx_hal::prelude::*;
use stm32h7xx_hal::{
    device::Peripherals,
    prelude::{_stm32h7xx_hal_delay_DelayExt, _stm32h7xx_hal_gpio_GpioExt},
    pwr::PwrExt,
    rcc::RccExt,
};
#[entry]
fn main() -> ! {
    let cp = cortex_m::Peripherals::take().unwrap();
    let dp = Peripherals::take().unwrap();

    let pwr = dp.PWR.constrain();
    let pwrcfg = pwr.vos3().freeze();

    let rcc = dp.RCC.constrain();
    let ccdr = rcc.sys_ck(100_u32.MHz()).freeze(pwrcfg, &dp.SYSCFG);

    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);

    let mut led = gpioc.pc13.into_push_pull_output();

    let mut delay = cp.SYST.delay(ccdr.clocks);
    loop {
        delay.delay_ms(500_u16);
        led.toggle();
    }
}
