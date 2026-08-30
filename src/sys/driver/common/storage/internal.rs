// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::cmp::min;
use core::ptr;

use embedded_storage::nor_flash::{
    check_erase, check_read, check_write, ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash,
};
use stm32_metapac as pac;

pub const SETTINGS_START: u32 = 0x0801_e000;

#[cfg(feature = "launchpad-pro")]
pub const SETTINGS_SIZE: u32 = 6 * 1024;
#[cfg(not(feature = "launchpad-pro"))]
pub const SETTINGS_SIZE: u32 = 8 * 1024;

pub const PAGE_SIZE: usize = 1024;

const FLASH_KEY1: u32 = 0x4567_0123;
const FLASH_KEY2: u32 = 0xcdef_89ab;

pub struct Flash {
    page_buf: [u8; PAGE_SIZE],
}

impl Flash {
    pub const fn new() -> Self {
        Self {
            page_buf: [0xff; PAGE_SIZE],
        }
    }

    pub const fn settings_size(&self) -> u32 {
        SETTINGS_SIZE
    }

    pub fn read_settings(&mut self, offset: u32, data: &mut [u8]) {
        if data.is_empty() {
            return;
        }
        if self.read(offset, data).is_err() {
            data.fill(0xff);
        }
    }

    pub fn write_settings(&mut self, offset: u32, data: &[u8]) {
        let _ = self.write(offset, data);
    }
}

impl ErrorType for Flash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for Flash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, data: &mut [u8]) -> Result<(), Self::Error> {
        check_read(self, offset, data.len())?;
        read_memory(offset, data);
        Ok(())
    }

    fn capacity(&self) -> usize {
        SETTINGS_SIZE as usize
    }
}

impl NorFlash for Flash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = PAGE_SIZE;

    fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), Self::Error> {
        check_write(self, offset, data.len())?;

        cortex_m::interrupt::free(|_| {
            unlock();
            let result = self.write_inner(offset, data);
            lock();
            result
        })
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        check_erase(self, from, to)?;

        cortex_m::interrupt::free(|_| {
            unlock();
            let result = (from..to)
                .step_by(PAGE_SIZE)
                .try_for_each(|offset| erase_page(SETTINGS_START + offset));
            lock();
            result
        })
    }
}

impl Flash {
    fn write_inner(&mut self, offset: u32, data: &[u8]) -> Result<(), NorFlashErrorKind> {
        let mut rel_off = offset;
        let mut src = data;

        while !src.is_empty() {
            let page_base = rel_off & !((PAGE_SIZE as u32) - 1);
            let in_page = (rel_off - page_base) as usize;
            let chunk = min(src.len(), PAGE_SIZE - in_page);

            read_memory(page_base, &mut self.page_buf);
            if self.page_buf[in_page..in_page + chunk] != src[..chunk] {
                self.page_buf[in_page..in_page + chunk].copy_from_slice(&src[..chunk]);
                erase_page(SETTINGS_START + page_base)?;
                program_page(SETTINGS_START + page_base, &self.page_buf)?;
            }

            rel_off += chunk as u32;
            src = &src[chunk..];
        }

        Ok(())
    }
}

fn read_memory(offset: u32, data: &mut [u8]) {
    let src = (SETTINGS_START + offset) as *const u8;
    unsafe {
        ptr::copy_nonoverlapping(src, data.as_mut_ptr(), data.len());
    }
}

fn unlock() {
    if !pac::FLASH.cr().read().lock() {
        return;
    }

    pac::FLASH.keyr().write_value(FLASH_KEY1);
    pac::FLASH.keyr().write_value(FLASH_KEY2);
}

fn lock() {
    pac::FLASH.cr().modify(|w| w.set_lock(true));
}

fn wait_ready() -> Result<(), NorFlashErrorKind> {
    while pac::FLASH.sr().read().bsy() {}

    let status = pac::FLASH.sr().read();
    let failed = status.pgerr() || status.wrprterr();
    pac::FLASH.sr().write(|w| {
        w.set_eop(true);
        w.set_pgerr(true);
        w.set_wrprterr(true);
    });

    if failed {
        Err(NorFlashErrorKind::Other)
    } else {
        Ok(())
    }
}

fn erase_page(address: u32) -> Result<(), NorFlashErrorKind> {
    wait_ready()?;
    pac::FLASH.cr().write(|w| w.set_per(true));
    pac::FLASH.ar().write(|w| w.set_far(address));
    pac::FLASH.cr().modify(|w| w.set_strt(true));
    let result = wait_ready();
    pac::FLASH.cr().write(|_| {});
    result
}

fn program_page(address: u32, data: &[u8; PAGE_SIZE]) -> Result<(), NorFlashErrorKind> {
    wait_ready()?;
    pac::FLASH.cr().write(|w| w.set_pg(true));

    let result = (|| {
        for (index, bytes) in data.chunks_exact(2).enumerate() {
            let halfword = u16::from_le_bytes([bytes[0], bytes[1]]);
            if halfword != 0xffff {
                unsafe {
                    ptr::write_volatile((address as usize + index * 2) as *mut u16, halfword);
                }
                wait_ready()?;
            }
        }
        Ok(())
    })();

    pac::FLASH.cr().write(|_| {});
    result
}
