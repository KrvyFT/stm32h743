#![no_main]
#![no_std]

mod flash;

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use embedded_hal::blocking::delay::DelayMs;
use fugit::RateExtU32;
use panic_semihosting as _;

use stm32h7xx_hal::{
    delay::DelayExt,
    device::Peripherals,
    gpio::{GpioExt, Speed::VeryHigh},
    pac,
    pwr::PwrExt,
    rcc::RccExt,
    serial::{config::Config, SerialExt},
    xspi::{self, Qspi, QspiWord, XspiExt},
};

use crate::flash::Flash;

const WRITE_ENABLE_CMD: u8 = 0x06;
const WRITE_DISABLE_CMD: u8 = 0x04;
const READ_STATUS_REGISTRY_CMD: u8 = 0x05;
const WRITE_STATUS_REGISTRY_CMD: u8 = 0x01;
const READ_CMD: u8 = 0x03;
const HREAD_CMD: u8 = 0x0B;
const HREADDO_CMD: u8 = 0x3B;
const WRITE_CMD: u8 = 0x02;
const SE_CMD: u8 = 0x20;
const BE_CMD: u8 = 0xD8;
const CE_CMD: u8 = 0xC7;
const POWERD_CMD: u8 = 0xB9;
const READ_ID_CMD: u8 = 0x90;
const READ_JEDECID_CMD: u8 = 0x9F;
const COUNTER_ADDRESS: u32 = 0;
#[entry]
fn main() -> ! {
    let (cp, dp) = init();

    let rcc = dp.RCC.constrain();

    let pwr = dp.PWR.constrain();
    let pwrcfg = pwr.vos3().freeze();

    let ccdr = rcc
        .sys_ck(128_u32.MHz())
        .pll1_q_ck(48.MHz())
        .freeze(pwrcfg, &dp.SYSCFG);
    let mut delay = cp.SYST.delay(ccdr.clocks);
    hprintln!("Init");
    // GPIO
    let gpioa = dp.GPIOA.split(ccdr.peripheral.GPIOA);
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);
    let gpiod = dp.GPIOD.split(ccdr.peripheral.GPIOD);
    let gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);

    // USART
    let tx = gpioa.pa9.into_alternate();
    let rx = gpioa.pa10.into_alternate();
    let config = Config::new(9600.Hz());
    let serial = dp
        .USART1
        .serial((tx, rx), config, ccdr.peripheral.USART1, &ccdr.clocks)
        .unwrap();
    let (mut tx, _) = serial.split();

    // LED
    let mut led = gpioc.pc13.into_push_pull_output();

    // SPI Flash
    let _qspi_cs = gpiob.pb6.into_alternate::<10>().speed(VeryHigh);

    let sck = gpiob.pb2.into_alternate().speed(VeryHigh);
    let io1 = gpiod.pd12.into_alternate().speed(VeryHigh);
    let io2 = gpioe.pe2.into_alternate().speed(VeryHigh);
    let io3 = gpiod.pd13.into_alternate().speed(VeryHigh);
    let io0 = gpiod.pd11.into_alternate().speed(VeryHigh);

    let mut qspi = dp.QUADSPI.bank1(
        (sck, io0, io1, io2, io3),
        3.MHz(),
        &ccdr.clocks,
        ccdr.peripheral.QSPI,
    );
    qspi.configure_mode(xspi::QspiMode::OneBit).unwrap();
    let mut flash = Flash::new(qspi);
    flash.rest_status_registry();
    let buf = b"ZXCVBNM";
    let address = 0x00;
    flash.erase(address);
    flash.write(address, buf).unwrap();
    let mut rbuf: [u8; 16] = [0; 16];
    flash.erase_chip().unwrap();
    flash.read(address, &mut rbuf).unwrap();
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

fn rest_status_registry(qspi: &mut Qspi<pac::QUADSPI>) {
    qspi.write_extended(
        xspi::QspiWord::U8(WRITE_STATUS_REGISTRY_CMD),
        xspi::QspiWord::U8(0b00000000),
        xspi::QspiWord::None,
        &[],
    )
    .unwrap();
}

fn enable_write_operation(qspi: &mut Qspi<pac::QUADSPI>) {
    qspi.write_extended(
        xspi::QspiWord::U8(WRITE_ENABLE_CMD),
        xspi::QspiWord::None,
        xspi::QspiWord::None,
        &[],
    )
    .unwrap();
}

fn read_u32(qspi: &mut Qspi<pac::QUADSPI>, address: u32) -> u32 {
    let mut buffer: [u8; 4] = [0xFF; 4];
    qspi.read_extended(
        QspiWord::U8(READ_CMD),
        QspiWord::U24(address),
        QspiWord::None,
        0,
        &mut buffer,
    )
    .unwrap();
    u32::from_be_bytes(buffer)
}

fn write_u32(qspi: &mut Qspi<pac::QUADSPI>, address: u32, value: u32) {
    enable_write_operation(qspi);
    qspi.write_extended(
        QspiWord::U8(SE_CMD),
        QspiWord::U24(address),
        QspiWord::None,
        &[],
    )
    .unwrap();

    wait_for_write_finish(qspi);

    let bytes: [u8; 4] = value.to_be_bytes();
    enable_write_operation(qspi);
    qspi.write_extended(
        QspiWord::U8(WRITE_CMD),
        QspiWord::U24(address),
        QspiWord::None,
        &bytes,
    )
    .unwrap();
    wait_for_write_finish(qspi);
}

fn wait_for_write_finish(qspi: &mut Qspi<pac::QUADSPI>) {
    loop {
        let mut status: [u8; 1] = [0xFF; 1];
        qspi.read_extended(
            QspiWord::U8(READ_STATUS_REGISTRY_CMD),
            QspiWord::None,
            QspiWord::None,
            0,
            &mut status,
        )
        .unwrap();

        if status[0] & 0x01 == 0 {
            break;
        }
    }
}
