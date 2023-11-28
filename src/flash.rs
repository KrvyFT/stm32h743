use core::fmt::Debug;
use stm32h7xx_hal::{
    device::bdma::ch,
    pac,
    xspi::{self, Qspi},
};

// 每页大小为 256 字节
pub const PAGE_SIZE: u32 = 256;
// 总共有 32768 页
pub const N_PAGES: u32 = 32768;
// 总容量为 PAGE_SIZE * N_PAGES 字节W
pub const CAPACITY: u32 = PAGE_SIZE * N_PAGES;
// 每扇区大小为 PAGE_SIZE * 16 字节
pub const SECTOR_SIZE: u32 = PAGE_SIZE * 16;
// 总共有 N_PAGES / 16 个扇区
pub const N_SECTORS: u32 = N_PAGES / 16;
// 每块 32KB 大小为 SECTOR_SIZE * 8 字节
pub const BLOCK_32K_SIZE: u32 = SECTOR_SIZE * 8;
// 总共有 N_SECTORS / 8 个 32KB 块
pub const N_BLOCKS_32K: u32 = N_SECTORS / 8;
// 每块 64KB 大小为 BLOCK_32K_SIZE * 2 字节
pub const BLOCK_64K_SIZE: u32 = BLOCK_32K_SIZE * 2;
// 总共有 N_BLOCKS_32K / 2 个 64KB 块
pub const N_BLOCKS_64K: u32 = N_BLOCKS_32K / 2;

#[derive(Debug)]
pub enum Error {
    NotAligned,
    OutOfBounds,
    WriteEnableFail,
    ReadbackFail,
}

#[repr(u8)]
pub enum Control {
    SectorErase = 0x20,
    Block32Erase = 0x52,
    Block64Erase = 0xD8,
    ChipErase = 0xC7,
    EnableReset = 0x66,
    Reset = 0x99,
    PowerDown = 0xB9,
    ReleasePowerDown = 0xAB,
    UniqueId = 0x4b,
    WriteEnable = 0x06,
    WriteDisable = 0x04,
    WriteStatusReg = 0x01,
    WritePage = 0x02,
    WriteFourPage = 0x32,
    ReadStatusReg1 = 0x05,
    ReadStatusReg2 = 0x35,
    ReadData = 0x03,
}
pub use Control::*;

pub struct Flash(pub Qspi<pac::QUADSPI>);

impl Flash {
    pub fn new(qspi: Qspi<pac::QUADSPI>) -> Self {
        Self(qspi)
    }

    fn read_status_register(&mut self) -> u8 {
        let mut buffer: [u8; 1] = [0u8; 1];
        self.0
            .read_extended(
                xspi::QspiWord::U8(ReadStatusReg1 as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                0,
                &mut buffer,
            )
            .unwrap();
        buffer[0]
    }

    fn busy(&mut self) -> bool {
        (self.read_status_register() & 0x01) != 0
    }

    fn write_enabled(&mut self) -> bool {
        (self.read_status_register() & 0x02) != 0
    }

    pub fn device_id(&mut self) -> u8 {
        let mut buffer: [u8; 1] = [0; 1];
        self.0
            .read_extended(
                xspi::QspiWord::U8(UniqueId as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                0,
                &mut buffer,
            )
            .unwrap();

        buffer[0]
    }

    pub fn rest(&mut self) {
        self.0
            .write_extended(
                xspi::QspiWord::U8(EnableReset as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();
        self.0
            .write_extended(
                xspi::QspiWord::U8(Reset as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();
    }

    pub fn read(&mut self, address: u32, buf: &mut [u8]) -> Result<(), Error> {
        if address + buf.len() as u32 > CAPACITY {
            return Err(Error::OutOfBounds);
        }

        self.0
            .read_extended(
                xspi::QspiWord::U8(ReadData as u8),
                xspi::QspiWord::U24(address),
                xspi::QspiWord::None,
                0,
                buf,
            )
            .unwrap();

        Ok(())
    }

    pub fn enable_write(&mut self) -> Result<(), Error> {
        self.0
            .write_extended(
                xspi::QspiWord::U8(WriteEnable as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();

        if !self.write_enabled() {
            return Err(Error::WriteEnableFail);
        }

        Ok(())
    }

    pub fn write_page(&mut self, address: u32, buf: &[u8]) -> Result<(), Error> {
        if (address & 0x000000FF) + buf.len() as u32 > PAGE_SIZE {
            return Err(Error::OutOfBounds);
        }
        self.enable_write().unwrap();
        self.0
            .write_extended(
                xspi::QspiWord::U8(WritePage as u8),
                xspi::QspiWord::U24(address),
                xspi::QspiWord::None,
                buf,
            )
            .unwrap();
        while self.busy() {}

        Ok(())
    }

    pub fn write(&mut self, mut address: u32, mut buf: &[u8]) -> Result<(), Error> {
        if address + buf.len() as u32 > CAPACITY {
            return Err(Error::OutOfBounds);
        }

        let chunk_len = (PAGE_SIZE - (address & 0x000000FF)) as usize;
        let chunk_len = chunk_len.min(buf.len());
        self.write_page(address, &buf[..chunk_len]).unwrap();
        let mut chunk_len = chunk_len;
        loop {
            buf = &buf[chunk_len..];

            address += chunk_len as u32;
            chunk_len = buf.len().min(PAGE_SIZE as usize);
            if chunk_len == 0 {
                break;
            }
            self.write_page(address, &buf[chunk_len..]).unwrap();
        }
        Ok(())
    }

    pub fn rest_status_registry(&mut self) {
        self.0
            .write_extended(
                xspi::QspiWord::U8(WriteStatusReg as u8),
                xspi::QspiWord::U8(0b00000000),
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();
    }

    pub fn erase_chip(&mut self) -> Result<(), Error> {
        self.enable_write()?;

        self.0
            .write_extended(
                xspi::QspiWord::U8(ChipErase as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();

        while self.busy() {}

        Ok(())
    }

    pub fn erase(&mut self, address: u32) {
        self.enable_write().unwrap();
        self.0
            .write_extended(
                xspi::QspiWord::U8(SectorErase as u8),
                xspi::QspiWord::U24(address),
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();

        while self.busy() {}
    }
}
