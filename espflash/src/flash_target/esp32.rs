use crate::connection::Connection;
use crate::elf::{FirmwareImage, RomSegment};
use crate::error::Error;
use crate::flash_target::{begin_command, block_command_with_timeout, FlashTarget};
use crate::flasher::{Command, SpiAttachParams, FLASH_SECTOR_SIZE, FLASH_WRITE_SIZE};
use crate::Chip;
use indicatif::{ProgressBar, ProgressStyle};

pub struct Esp32Target {
    chip: Chip,
    spi_attach_params: SpiAttachParams,
}

impl Esp32Target {
    pub fn new(chip: Chip, spi_attach_params: SpiAttachParams) -> Self {
        Esp32Target {
            chip,
            spi_attach_params,
        }
    }
}

impl FlashTarget for Esp32Target {
    fn begin(&mut self, connection: &mut Connection, _image: &FirmwareImage) -> Result<(), Error> {
        let spi_params = self.spi_attach_params.encode();
        connection.with_timeout(Command::SpiAttach.timeout(), |connection| {
            connection.command(Command::SpiAttach as u8, spi_params.as_slice(), 0)
        })?;
        Ok(())
    }

    fn write_segment(
        &mut self,
        connection: &mut Connection,
        segment: &RomSegment,
    ) -> Result<(), Error> {
        let addr = segment.addr;
        let erase_count = (segment.data.len() + FLASH_SECTOR_SIZE - 1) / FLASH_SECTOR_SIZE;

        // round up to sector size
        let erase_size = (erase_count * FLASH_SECTOR_SIZE) as u32;

        begin_command(
            connection,
            Command::FlashBegin,
            erase_size,
            erase_count as u32,
            FLASH_WRITE_SIZE as u32,
            addr,
            self.chip != Chip::Esp32,
        )?;

        let chunks = segment.data.chunks(FLASH_WRITE_SIZE);

        let (_, chunk_size) = chunks.size_hint();
        let chunk_size = chunk_size.unwrap_or(0) as u64;
        let pb_chunk = ProgressBar::new(chunk_size);
        pb_chunk.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
                .progress_chars("#>-"),
        );

        for (i, block) in chunks.enumerate() {
            pb_chunk.set_message(format!("segment 0x{:X} writing chunks", addr));
            block_command_with_timeout(
                connection,
                Command::FlashData,
                block,
                0,
                0xff,
                i as u32,
                Command::FlashData.timeout_for_size(block.len() as u32),
            )?;
            pb_chunk.inc(1);
        }

        pb_chunk.finish_with_message(format!("segment 0x{:X}", addr));

        Ok(())
    }

    fn finish(&mut self, connection: &mut Connection, reboot: bool) -> Result<(), Error> {
        connection.with_timeout(Command::FlashEnd.timeout(), |connection| {
            connection.write_command(Command::FlashEnd as u8, &[1][..], 0)
        })?;
        if reboot {
            connection.reset()
        } else {
            Ok(())
        }
    }
}
