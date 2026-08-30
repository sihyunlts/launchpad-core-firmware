// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU8, Ordering, compiler_fence};

use cortex_m::register::{basepri, basepri_max};
use embassy_stm32::interrupt::{self, InterruptExt, Priority};

use super::grid::Grid;

const LED_PLANE_SIZE: usize = 256;
const MK2_KEY_COUNT: usize = 80;
const GROUP_COUNT: usize = 4;
const GROUP_STRIDE: usize = 64;
const SHIFT_BYTES_PER_GROUP: usize = 8;

const INDEX_TO_KEY: [u8; 110] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 63, 64, 65, 66, 67, 68, 69,
    70, 71, 0xff, 54, 55, 56, 57, 58, 59, 60, 61, 62, 0xff, 45, 46, 47, 48, 49, 50, 51, 52, 53,
    0xff, 36, 37, 38, 39, 40, 41, 42, 43, 44, 0xff, 27, 28, 29, 30, 31, 32, 33, 34, 35, 0xff, 18,
    19, 20, 21, 22, 23, 24, 25, 26, 0xff, 9, 10, 11, 12, 13, 14, 15, 16, 17, 0xff, 0, 1, 2, 3, 4,
    5, 6, 7, 8, 0xff, 72, 73, 74, 75, 76, 77, 78, 79, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff,
];

const BLUE_MAP: [u8; MK2_KEY_COUNT] = [
    0x39, 0x79, 0xb9, 0xf9, 0x36, 0x76, 0xb6, 0xf6, 0x27, 0x33, 0x73, 0xb3, 0xf3, 0x30, 0x70, 0xb0,
    0xf0, 0x67, 0x2d, 0x6d, 0xad, 0xed, 0x2a, 0x6a, 0xaa, 0xea, 0xa7, 0x1f, 0x5f, 0x9f, 0xdf, 0x1c,
    0x5c, 0x9c, 0xdc, 0xe7, 0x19, 0x59, 0x99, 0xd9, 0x16, 0x56, 0x96, 0xd6, 0x24, 0x13, 0x53, 0x93,
    0xd3, 0x10, 0x50, 0x90, 0xd0, 0x64, 0x0d, 0x4d, 0x8d, 0xcd, 0x0a, 0x4a, 0x8a, 0xca, 0xa4, 0x07,
    0x47, 0x87, 0xc7, 0x04, 0x44, 0x84, 0xc4, 0xe4, 0x3f, 0x7f, 0xbf, 0xff, 0x3c, 0x7c, 0xbc, 0xfc,
];
const RED_MAP: [u8; MK2_KEY_COUNT] = [
    0x38, 0x78, 0xb8, 0xf8, 0x35, 0x75, 0xb5, 0xf5, 0x26, 0x32, 0x72, 0xb2, 0xf2, 0x2f, 0x6f, 0xaf,
    0xef, 0x66, 0x2c, 0x6c, 0xac, 0xec, 0x29, 0x69, 0xa9, 0xe9, 0xa6, 0x1e, 0x5e, 0x9e, 0xde, 0x1b,
    0x5b, 0x9b, 0xdb, 0xe6, 0x18, 0x58, 0x98, 0xd8, 0x15, 0x55, 0x95, 0xd5, 0x23, 0x12, 0x52, 0x92,
    0xd2, 0x0f, 0x4f, 0x8f, 0xcf, 0x63, 0x0c, 0x4c, 0x8c, 0xcc, 0x09, 0x49, 0x89, 0xc9, 0xa3, 0x06,
    0x46, 0x86, 0xc6, 0x03, 0x43, 0x83, 0xc3, 0xe3, 0x3e, 0x7e, 0xbe, 0xfe, 0x3b, 0x7b, 0xbb, 0xfb,
];
const GREEN_MAP: [u8; MK2_KEY_COUNT] = [
    0x37, 0x77, 0xb7, 0xf7, 0x34, 0x74, 0xb4, 0xf4, 0x25, 0x31, 0x71, 0xb1, 0xf1, 0x2e, 0x6e, 0xae,
    0xee, 0x65, 0x2b, 0x6b, 0xab, 0xeb, 0x28, 0x68, 0xa8, 0xe8, 0xa5, 0x1d, 0x5d, 0x9d, 0xdd, 0x1a,
    0x5a, 0x9a, 0xda, 0xe5, 0x17, 0x57, 0x97, 0xd7, 0x14, 0x54, 0x94, 0xd4, 0x22, 0x11, 0x51, 0x91,
    0xd1, 0x0e, 0x4e, 0x8e, 0xce, 0x62, 0x0b, 0x4b, 0x8b, 0xcb, 0x08, 0x48, 0x88, 0xc8, 0xa2, 0x05,
    0x45, 0x85, 0xc5, 0x02, 0x42, 0x82, 0xc2, 0xe2, 0x3d, 0x7d, 0xbd, 0xfd, 0x3a, 0x7a, 0xba, 0xfa,
];

const RAW_RED_OFFSET: usize = 0;
const RAW_GREEN_OFFSET: usize = MK2_KEY_COUNT;
const RAW_BLUE_OFFSET: usize = MK2_KEY_COUNT * 2;
const RAW_RGB_SIZE: usize = MK2_KEY_COUNT * 3;

fn apply_led_brightness(value: u8, brightness: u8) -> u8 {
    let value = value.min(0x3f);
    if value == 0 {
        return 0;
    }

    let x = brightness.min(7) as u32;
    ((((value as u32 - 1) * (63 * (x + 5) * (x + 5) - 144)) / (62 * 144)) + 1) as u8
}

pub struct Leds {
    plane: [u8; LED_PLANE_SIZE],
    raw_rgb: [u8; RAW_RGB_SIZE],
    brightness: u8,
}

impl Leds {
    pub fn new() -> Self {
        Self {
            plane: [0; LED_PLANE_SIZE],
            raw_rgb: [0; RAW_RGB_SIZE],
            brightness: 7,
        }
    }

    pub fn clear(&mut self) {
        self.fill(0);
    }

    pub fn fill(&mut self, rgb: u32) {
        for key in 0..MK2_KEY_COUNT {
            self.set_key_led(key, rgb);
        }
    }

    pub fn set_led(&mut self, index: u8, rgb: u32) {
        if let Some(key) = key_from_index(index) {
            self.set_key_led(key as usize, rgb);
        }
    }

    pub fn set_led_rgb(&mut self, index: u8, r: u8, g: u8, b: u8) {
        if let Some(key) = key_from_index(index) {
            self.set_key_rgb(key as usize, r, g, b);
        }
    }

    pub fn build_group_payload(&self, group: usize, bright_bit: usize, out: &mut [u8; 8]) {
        out.fill(0xff);
        if group >= GROUP_COUNT || bright_bit >= 8 {
            return;
        }

        let mask = 1u8 << bright_bit;
        let base = group * GROUP_STRIDE;

        for (byte_idx, dst) in out.iter_mut().enumerate().take(SHIFT_BYTES_PER_GROUP) {
            let mut bits = 0u8;
            let src = base + byte_idx * 8;
            for lane in 0..8 {
                if self.plane[src + lane] & mask != 0 {
                    bits |= 1 << lane;
                }
            }
            *dst = !bits;
        }
    }

    pub fn brightness(&self) -> u8 {
        self.brightness
    }

    pub fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness.min(7);
        self.rebuild_scaled_plane();
    }

    fn set_key_led(&mut self, key: usize, rgb: u32) {
        let r = ((rgb >> 18) & 0x3f) as u8;
        let g = ((rgb >> 10) & 0x3f) as u8;
        let b = ((rgb >> 2) & 0x3f) as u8;
        self.set_key_rgb(key, r, g, b);
    }

    fn set_key_rgb(&mut self, key: usize, r: u8, g: u8, b: u8) {
        if key >= MK2_KEY_COUNT {
            return;
        }

        self.raw_rgb[RAW_RED_OFFSET + key] = r.min(0x3f);
        self.raw_rgb[RAW_GREEN_OFFSET + key] = g.min(0x3f);
        self.raw_rgb[RAW_BLUE_OFFSET + key] = b.min(0x3f);
        self.write_scaled_key(key);
    }

    fn write_scaled_key(&mut self, key: usize) {
        self.plane[RED_MAP[key] as usize] =
            self.scale_intensity(self.raw_rgb[RAW_RED_OFFSET + key]);
        self.plane[GREEN_MAP[key] as usize] =
            self.scale_intensity(self.raw_rgb[RAW_GREEN_OFFSET + key]);
        self.plane[BLUE_MAP[key] as usize] =
            self.scale_intensity(self.raw_rgb[RAW_BLUE_OFFSET + key]);
    }

    fn rebuild_scaled_plane(&mut self) {
        for key in 0..MK2_KEY_COUNT {
            self.write_scaled_key(key);
        }
    }

    fn scale_intensity(&self, value: u8) -> u8 {
        apply_led_brightness(value, self.brightness)
    }
}

fn key_from_index(index: u8) -> Option<u8> {
    let &key = INDEX_TO_KEY.get(index as usize)?;
    if key == 0xff { None } else { Some(key) }
}

const RCC_APB1ENR: *mut u32 = 0x4002_101c as *mut u32;
const TIM2_CR1: *mut u32 = 0x4000_0000 as *mut u32;
const TIM2_DIER: *mut u32 = 0x4000_000c as *mut u32;
const TIM2_SR: *mut u32 = 0x4000_0010 as *mut u32;
const TIM2_EGR: *mut u32 = 0x4000_0014 as *mut u32;
const TIM2_CNT: *mut u32 = 0x4000_0024 as *mut u32;
const TIM2_PSC: *mut u32 = 0x4000_0028 as *mut u32;
const TIM2_ARR: *mut u32 = 0x4000_002c as *mut u32;

const RCC_APB1ENR_TIM2EN: u32 = 1 << 0;
const TIM_CR1_CEN: u32 = 1 << 0;
const TIM_DIER_UIE: u32 = 1 << 0;
const TIM_SR_UIF: u32 = 1 << 0;
const TIM_EGR_UG: u32 = 1 << 0;
const TIM_CR1_MK2_BASE: u32 = 0x0014;

const BLANK_PHASE_TICKS: u16 = 20;
const SHIFT_SETTLE_TICKS: u16 = 2;
const BRIGHT_TIMES_USB_POWER: [u16; 6] = [2, 374, 14, 182, 38, 86];
const MIN_IRQ_INTERVAL_TICKS: u16 = 2;

static GRID: AtomicPtr<Grid> = AtomicPtr::new(ptr::null_mut());
static SURFACE_PHASE: AtomicU8 = AtomicU8::new(0);

pub fn start_scan(grid: *mut Grid) {
    GRID.store(grid, Ordering::Release);
    SURFACE_PHASE.store(0, Ordering::Relaxed);

    unsafe {
        modify_reg(RCC_APB1ENR, |value| value | RCC_APB1ENR_TIM2EN);
        write_reg(TIM2_CR1, TIM_CR1_MK2_BASE);
        write_reg(TIM2_PSC, 48 - 1);
        write_reg(TIM2_CNT, BLANK_PHASE_TICKS as u32);
        write_reg(TIM2_ARR, BLANK_PHASE_TICKS as u32);
        write_reg(TIM2_EGR, TIM_EGR_UG);
        write_reg(TIM2_SR, 0);
        write_reg(TIM2_DIER, TIM_DIER_UIE);
    }

    interrupt::TIM2.set_priority(Priority::P0);
    interrupt::TIM2.unpend();
    unsafe {
        interrupt::TIM2.enable();
    }

    unsafe {
        modify_reg(TIM2_CR1, |value| value | TIM_CR1_CEN);
    }
}

#[cortex_m_rt::interrupt]
fn TIM2() {
    if unsafe { read_reg(TIM2_SR) } & TIM_SR_UIF == 0 {
        return;
    }

    let grid = GRID.load(Ordering::Acquire);
    if grid.is_null() {
        unsafe {
            write_reg(TIM2_SR, 0);
        }
        return;
    }
    let grid = unsafe { &mut *grid };

    let phase = SURFACE_PHASE.load(Ordering::Relaxed);
    let next_delay = match phase {
        0 => {
            grid.blank_phase();
            BLANK_PHASE_TICKS
        }
        1 => {
            grid.ledshift_phase();
            SHIFT_SETTLE_TICKS
        }
        _ => {
            let bright_step = grid.bright_phase() as usize;
            BRIGHT_TIMES_USB_POWER[bright_step]
        }
    };

    SURFACE_PHASE.store((phase + 1) % 3, Ordering::Relaxed);
    unsafe {
        let ticks = timer_arr_from_ticks(next_delay) as u32;
        write_reg(TIM2_ARR, ticks);
        write_reg(TIM2_CNT, ticks);
        write_reg(TIM2_SR, 0);
    }
}

fn timer_arr_from_ticks(ticks: u16) -> u16 {
    ticks.max(MIN_IRQ_INTERVAL_TICKS)
}

unsafe fn read_reg(reg: *mut u32) -> u32 {
    unsafe { ptr::read_volatile(reg) }
}

unsafe fn write_reg(reg: *mut u32, value: u32) {
    unsafe {
        ptr::write_volatile(reg, value);
    }
}

unsafe fn modify_reg(reg: *mut u32, f: impl FnOnce(u32) -> u32) {
    unsafe {
        let value = ptr::read_volatile(reg);
        ptr::write_volatile(reg, f(value));
    }
}

const RESERVE_P0_BASEPRI: u8 = Priority::P1 as u8;

struct ScanPriorityCriticalSection;
critical_section::set_impl!(ScanPriorityCriticalSection);

unsafe impl critical_section::Impl for ScanPriorityCriticalSection {
    unsafe fn acquire() -> critical_section::RawRestoreState {
        let previous = basepri::read();

        basepri_max::write(RESERVE_P0_BASEPRI);
        compiler_fence(Ordering::SeqCst);
        previous
    }

    unsafe fn release(previous: critical_section::RawRestoreState) {
        compiler_fence(Ordering::SeqCst);
        // SAFETY: `previous` is the exact `BASEPRI` value captured by the
        // matching `acquire`, so this only ever restores the prior level.
        unsafe {
            basepri::write(previous);
        }
    }
}
