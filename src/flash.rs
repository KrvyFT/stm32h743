use core::fmt::Debug;
use fugit::RateExtU32;
use stm32h7xx_hal::{
    gpio::{Alternate, Pin},
    pac,
    rcc::CoreClocks,
    xspi::{self, Qspi, XspiExt},
};

pub const PAGE_SIZE: u32 = 256;
pub const N_PAGES: u32 = 32768;
pub const CAPACITY: u32 = PAGE_SIZE * N_PAGES;
pub const SECTOR_SIZE: u32 = PAGE_SIZE * 16;
pub const N_SECTORS: u32 = N_PAGES / 16;
pub const BLOCK_32K_SIZE: u32 = SECTOR_SIZE * 8;
pub const N_BLOCKS_32K: u32 = N_SECTORS / 8;
pub const BLOCK_64K_SIZE: u32 = BLOCK_32K_SIZE * 2;
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
    UniqueId = 0x4B,
    WriteEnable = 0x06,
    WriteDisable = 0x04,
    WriteStatusReg = 0x01,
    WritePage = 0x02,
    // WriteFourPage = 0x32,
    ReadStatusReg1 = 0x05,
    ReadStatusReg2 = 0x35,
    ReadData = 0x03,
}
pub use Control::*;

pub struct Flash(Qspi<pac::QUADSPI>);

impl Flash {
    pub fn new(
        qad: pac::QUADSPI,
        sck: Pin<'B', 2, Alternate<9>>,
        io0: Pin<'D', 11, Alternate<9>>,
        io1: Pin<'D', 12, Alternate<9>>,
        io2: Pin<'E', 2, Alternate<9>>,
        io3: Pin<'D', 13, Alternate<9>>,
        clock: &CoreClocks,
        qspi: stm32h7xx_hal::rcc::rec::Qspi,
    ) -> Self {
        let mut qspi = qad.bank1((sck, io0, io1, io2, io3), 80.MHz(), clock, qspi);
        qspi.configure_mode(xspi::QspiMode::OneBit).unwrap();
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
        self.erase(address);
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

        self.readback_check(address, &buf).unwrap();
        Ok(())
    }

    fn readback_check(&mut self, mut address: u32, data: &[u8]) -> Result<(), Error> {
        const CHUNK_SIZE: usize = 64;

        let mut buf = [0; CHUNK_SIZE];
        for chunk in data.chunks(CHUNK_SIZE) {
            let buf = &mut buf[..chunk.len()];
            self.read(address, buf).unwrap();
            address += CHUNK_SIZE as u32;

            if buf != chunk {
                return Err(Error::ReadbackFail);
            }
        }

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

    pub fn erase_range(&mut self, start: u32, end: u32) -> Result<(), Error> {
        if start % SECTOR_SIZE != 0 {
            return Err(Error::NotAligned);
        }
        if end % SECTOR_SIZE != 0 {
            return Err(Error::NotAligned);
        }
        if start > end {
            return Err(Error::OutOfBounds);
        }

        for sector in start..end {
            self.erase_sector(sector).unwrap();
        }
        Ok(())
    }

    pub fn erase_sector(&mut self, index: u32) -> Result<(), Error> {
        if index > N_SECTORS {
            return Err(Error::OutOfBounds);
        }
        self.enable_write().unwrap();
        let address = index * SECTOR_SIZE;
        self.0
            .write_extended(
                xspi::QspiWord::U8(SectorErase as u8),
                xspi::QspiWord::U24(address),
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();
        while self.busy() {}

        for offset in (0..SECTOR_SIZE).step_by(64) {
            self.readback_check(address + offset, &[0xFF; 64])?;
        }

        Ok(())
    }

    pub fn erase_block_32k(&mut self, index: u32) -> Result<(), Error> {
        if index >= N_BLOCKS_32K {
            return Err(Error::OutOfBounds);
        }

        self.enable_write()?;

        let address = index * BLOCK_32K_SIZE;

        self.0
            .write_extended(
                xspi::QspiWord::U8(Block32Erase as u8),
                xspi::QspiWord::U24(address),
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();

        for offset in (0..BLOCK_32K_SIZE).step_by(64) {
            self.readback_check(address + offset, &[0xFF; 64])?;
        }

        Ok(())
    }

    pub fn erase_block_64k(&mut self, index: u32) -> Result<(), Error> {
        if index >= N_BLOCKS_64K {
            return Err(Error::OutOfBounds);
        }

        self.enable_write()?;

        let address = index * BLOCK_64K_SIZE;

        self.0
            .write_extended(
                xspi::QspiWord::U8(Block64Erase as u8),
                xspi::QspiWord::U24(address),
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();

        for offset in (0..BLOCK_32K_SIZE).step_by(64) {
            self.readback_check(address + offset, &[0xFF; 64])?;
        }

        Ok(())
    }

    pub fn enable_power_down_mode(&mut self) {
        self.0
            .write_extended(
                xspi::QspiWord::U8(PowerDown as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();
    }

    pub fn disable_power_down_mode(&mut self) {
        self.0
            .write_extended(
                xspi::QspiWord::U8(ReleasePowerDown as u8),
                xspi::QspiWord::None,
                xspi::QspiWord::None,
                &[],
            )
            .unwrap();
    }
}
