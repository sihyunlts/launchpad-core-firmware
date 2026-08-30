// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

//! LED bit-plane buffer model + the TIM3-driven scan sequencer.
//!
//! This is a clean-room reimplementation of the reference Launchpad Pro
//! firmware's LED scan engine (`TIM3_IRQHandler` / `BLANK_func` /
//! `NULLSURFACE_func` / `LEDSHIFT_func` / `BRIGHT_func`), reverse engineered
//! from the original firmware disassembly. The critical fix versus a naive
//! reimplementation is that the "bright" (unblank + hold) phase is gated on
//! the SPI2/DMA shift-register transfer having actually completed
//! (`DMAFinished`), rather than assuming a fixed transfer time or blocking
//! inside the timer interrupt. Without this gate, low-brightness LEDs
//! (whose entire on-time is only 2-5 timer ticks) are extremely sensitive to
//! any scheduling jitter, which is what caused the visible flicker.

use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, Ordering, compiler_fence};

use cortex_m::register::{basepri, basepri_max};
use embassy_stm32::interrupt::{self, InterruptExt};
use stm32_metapac as pac;

use super::grid::Grid;

const LP_LED_COUNT: usize = 100;
const LED_STATUS_BYTES: usize = 320;
const GROUP_COUNT: usize = 4;
const BRIGHT_BIT_COUNT: usize = 6;
const GROUP_STRIDE: usize = 0x50;
const SHIFT_BYTES_PER_GROUP: usize = 10;

const GREEN_MAP: [u16; LP_LED_COUNT] = [
    0, 23, 103, 183, 263, 20, 100, 180, 260, 0, 250, 29, 109, 189, 269, 26, 106, 186, 266, 242,
    170, 35, 115, 195, 275, 32, 112, 192, 272, 162, 90, 42, 122, 202, 282, 36, 116, 196, 276, 82,
    10, 47, 127, 207, 287, 44, 124, 204, 284, 2, 253, 55, 135, 215, 295, 51, 131, 211, 291, 245,
    173, 56, 136, 216, 296, 57, 137, 217, 297, 165, 93, 66, 146, 226, 306, 60, 140, 220, 300, 85,
    13, 71, 151, 231, 311, 68, 148, 228, 308, 5, 0, 77, 157, 237, 317, 74, 154, 234, 314, 17,
];
const RED_MAP: [u16; LP_LED_COUNT] = [
    0, 24, 104, 184, 264, 21, 101, 181, 261, 0, 251, 30, 110, 190, 270, 27, 107, 187, 267, 243,
    171, 38, 118, 198, 278, 33, 113, 193, 273, 163, 91, 41, 121, 201, 281, 37, 117, 197, 277, 83,
    11, 48, 128, 208, 288, 45, 125, 205, 285, 3, 254, 54, 134, 214, 294, 50, 130, 210, 290, 246,
    174, 62, 142, 222, 302, 58, 138, 218, 298, 166, 94, 65, 145, 225, 305, 61, 141, 221, 301, 86,
    14, 72, 152, 232, 312, 69, 149, 229, 309, 6, 0, 78, 158, 238, 318, 75, 155, 235, 315, 18,
];
const BLUE_MAP: [u16; LP_LED_COUNT] = [
    0, 25, 105, 185, 265, 22, 102, 182, 262, 0, 252, 31, 111, 191, 271, 28, 108, 188, 268, 244,
    172, 39, 119, 199, 279, 34, 114, 194, 274, 164, 92, 40, 120, 200, 280, 43, 123, 203, 283, 84,
    12, 49, 129, 209, 289, 46, 126, 206, 286, 4, 255, 53, 133, 213, 293, 52, 132, 212, 292, 247,
    175, 63, 143, 223, 303, 59, 139, 219, 299, 167, 95, 64, 144, 224, 304, 67, 147, 227, 307, 87,
    15, 73, 153, 233, 313, 70, 150, 230, 310, 7, 0, 79, 159, 239, 319, 76, 156, 236, 316, 19,
];

const RAW_RED_OFFSET: usize = 0;
const RAW_GREEN_OFFSET: usize = LP_LED_COUNT;
const RAW_BLUE_OFFSET: usize = LP_LED_COUNT * 2;
const RAW_RGB_SIZE: usize = LP_LED_COUNT * 3;

fn apply_led_brightness(value: u8, brightness: u8) -> u8 {
    let value = value.min(0x3f);
    if value == 0 {
        return 0;
    }

    let x = brightness.min(7) as u32;
    ((((value as u32 - 1) * (63 * (x + 5) * (x + 5) - 144)) / (62 * 144)) + 1) as u8
}

pub struct Leds {
    payload: [[[u8; SHIFT_BYTES_PER_GROUP]; BRIGHT_BIT_COUNT]; GROUP_COUNT],
    raw_rgb: [u8; RAW_RGB_SIZE],
    brightness: u8,
}

impl Leds {
    pub fn new() -> Self {
        Self {
            payload: [[[0xff; SHIFT_BYTES_PER_GROUP]; BRIGHT_BIT_COUNT]; GROUP_COUNT],
            raw_rgb: [0; RAW_RGB_SIZE],
            brightness: 7,
        }
    }

    pub fn clear(&mut self) {
        self.fill(0);
    }

    pub fn fill(&mut self, rgb: u32) {
        for i in 0..LP_LED_COUNT {
            self.set_led(i as u8, rgb);
        }
    }

    pub fn set_led(&mut self, led: u8, rgb: u32) {
        if led as usize >= LP_LED_COUNT {
            return;
        }

        let rgb = rgb & 0x00ff_ffff;
        let r = ((rgb >> 18) & 0x3f) as u8;
        let g = ((rgb >> 10) & 0x3f) as u8;
        let b = ((rgb >> 2) & 0x3f) as u8;
        self.set_led_rgb(led, r, g, b);
    }

    pub fn set_led_rgb(&mut self, led: u8, r: u8, g: u8, b: u8) {
        if led as usize >= LP_LED_COUNT {
            return;
        }

        let led = led as usize;
        self.raw_rgb[RAW_RED_OFFSET + led] = r.min(0x3f);
        self.raw_rgb[RAW_GREEN_OFFSET + led] = g.min(0x3f);
        self.raw_rgb[RAW_BLUE_OFFSET + led] = b.min(0x3f);
        self.write_scaled_led(led);
    }

    pub fn build_group_payload(&self, group: usize, bright_bit: usize, out: &mut [u8; 10]) {
        if group >= GROUP_COUNT || bright_bit >= BRIGHT_BIT_COUNT {
            out.fill(0xff);
            return;
        }

        out.copy_from_slice(&self.payload[group][bright_bit]);
    }

    pub fn brightness(&self) -> u8 {
        self.brightness
    }

    pub fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness.min(7);
        self.rebuild_scaled_payload();
    }

    fn write_scaled_led(&mut self, led: usize) {
        let r = self.scale_intensity(self.raw_rgb[RAW_RED_OFFSET + led]);
        let g = self.scale_intensity(self.raw_rgb[RAW_GREEN_OFFSET + led]);
        let b = self.scale_intensity(self.raw_rgb[RAW_BLUE_OFFSET + led]);

        self.write_status(RED_MAP[led], r);
        self.write_status(GREEN_MAP[led], g);
        self.write_status(BLUE_MAP[led], b);
    }

    fn rebuild_scaled_payload(&mut self) {
        self.payload = [[[0xff; SHIFT_BYTES_PER_GROUP]; BRIGHT_BIT_COUNT]; GROUP_COUNT];
        for led in 0..LP_LED_COUNT {
            self.write_scaled_led(led);
        }
    }

    fn scale_intensity(&self, value: u8) -> u8 {
        apply_led_brightness(value, self.brightness)
    }

    fn write_status(&mut self, bit_index: u16, value: u8) {
        let offset = bit_index as usize;
        if offset >= LED_STATUS_BYTES {
            return;
        }

        let group = offset / GROUP_STRIDE;
        if group >= GROUP_COUNT {
            return;
        }

        let group_offset = offset - group * GROUP_STRIDE;
        let byte_index = group_offset >> 3;
        if byte_index >= SHIFT_BYTES_PER_GROUP {
            return;
        }

        let bit_mask = 1u8 << (group_offset & 7);
        for bright_bit in 0..BRIGHT_BIT_COUNT {
            let dst = &mut self.payload[group][bright_bit][byte_index];
            if value & (1 << bright_bit) != 0 {
                *dst &= !bit_mask;
            } else {
                *dst |= bit_mask;
            }
        }
    }
}

// --- TIM3 scan sequencer ---------------------------------------------------
//
// Reference `SurfaceMode` state machine (TIM3_IRQHandler):
//   0 = BLANK        - assert blank, advance (group, bright-bit), build the
//                       next shift-out payload
//   1 = NULLSURFACE  - (re-)arm the DMA transfer-count registers
//   2 = LEDSHIFT     - start the SPI2 TX/RX DMA transfer (non-blocking)
//   3 = BRIGHT       - once DMA has completed, release blank and select the
//                       current group for exactly `BRIGHT_TIMES` ticks
//
// Phase 3 is gated on `DMA_FINISHED`: if the previous DMA transfer hasn't
// completed yet, TIM3 is reloaded with a short `DMA_POLL_TICKS` interval and
// re-checks next time, rather than ever unblanking with stale/partial
// shift-register data.

const TIMER_VALUE: [u16; 3] = [10, 10, 50];
/// `BRIGHT_TIMES[bright_bit][power_mode - 1]`, on-time in TIM3 ticks for each
/// BCM bit-plane, for PowerMode 1 (bus-powered) and PowerMode 2
/// (self-powered). Values follow the exact reference law
/// `T(n) = U(power_mode) * (6*2^n - 5)` after re-indexing by bit-order.
const BRIGHT_TIMES: [[u16; 2]; BRIGHT_BIT_COUNT] = [
    [2, 5],
    [374, 935],
    [14, 35],
    [182, 455],
    [38, 95],
    [86, 215],
];
const DMA_POLL_TICKS: u16 = 5;
const TIM3_PERIOD_TICKS: u16 = 10;
const TIM3_PRESCALER: u16 = 59;

static GRID: AtomicPtr<Grid> = AtomicPtr::new(ptr::null_mut());
static SURFACE_MODE: AtomicU8 = AtomicU8::new(0);
static DMA_FINISHED: AtomicBool = AtomicBool::new(false);
static POWER_MODE: AtomicU8 = AtomicU8::new(1);

/// Called once by the VBUS-detect logic (`Grid`) to set the initial power
/// mode before scanning starts, and afterwards on every confirmed VBUS
/// transition.
pub fn set_power_mode(power_mode: u8) {
    POWER_MODE.store(power_mode.clamp(1, 2), Ordering::Relaxed);
}

pub fn start_scan(grid: *mut Grid) {
    GRID.store(grid, Ordering::Release);
    SURFACE_MODE.store(0, Ordering::Relaxed);
    DMA_FINISHED.store(false, Ordering::Relaxed);

    pac::RCC.apb1enr().modify(|w| w.set_tim3en(true));
    pac::TIM3.cr1().modify(|w| {
        w.set_cen(false);
        w.set_urs(pac::timer::vals::Urs::COUNTER_ONLY);
        w.set_dir(pac::timer::vals::Dir::DOWN);
    });
    pac::TIM3.psc().write_value(TIM3_PRESCALER);
    pac::TIM3
        .arr()
        .write_value(pac::timer::regs::ArrCore(TIM3_PERIOD_TICKS as u32));
    pac::TIM3
        .cnt()
        .write_value(pac::timer::regs::CntCore(TIMER_VALUE[0] as u32));
    pac::TIM3.egr().write(|w| w.set_ug(true));
    pac::TIM3.sr().write(|w| w.set_uif(false));
    pac::TIM3.dier().modify(|w| w.set_uie(true));

    // TIM3 drives the LED scan and must service its update interrupt with as
    // little jitter as possible: any latency at a "bright" phase boundary
    // directly stretches or clips that subframe's on-time, which is visible
    // as flicker (especially on the short low-order bit-planes). Give it the
    // highest priority so it preempts USB (P2), the embassy time driver and
    // the executor. The DMA-RX completion handler shares the same priority
    // (see below).
    interrupt::TIM3.set_priority(interrupt::Priority::P0);
    interrupt::TIM3.unpend();
    unsafe {
        interrupt::TIM3.enable();
    }

    // Match the reference firmware, which runs TIM3 and the DMA channel
    // interrupts all at preemption priority 0 (highest). Equal priority means
    // the DMA-RX completion handler that sets `DMAFinished` and the TIM3 scan
    // handler never preempt each other, keeping the bright-phase gate
    // deterministic.
    interrupt::DMA1_CHANNEL4.set_priority(interrupt::Priority::P0);
    interrupt::DMA1_CHANNEL4.unpend();
    unsafe {
        interrupt::DMA1_CHANNEL4.enable();
    }

    pac::TIM3.cr1().modify(|w| w.set_cen(true));
}

#[cortex_m_rt::interrupt]
fn TIM3() {
    if !pac::TIM3.sr().read().uif() {
        return;
    }
    pac::TIM3.sr().write(|w| w.set_uif(false));

    let grid = GRID.load(Ordering::Acquire);
    if grid.is_null() {
        return;
    }
    let grid = unsafe { &mut *grid };

    let mode = SURFACE_MODE.load(Ordering::Relaxed);

    if mode != 3 || DMA_FINISHED.load(Ordering::Acquire) {
        if mode == 3 && DMA_FINISHED.load(Ordering::Acquire) {
            DMA_FINISHED.store(false, Ordering::Release);
        }

        let next_delay = match mode {
            0 => {
                grid.blank_phase();
                TIMER_VALUE[0]
            }
            1 => {
                grid.null_surface_phase();
                TIMER_VALUE[1]
            }
            2 => {
                grid.ledshift_phase();
                TIMER_VALUE[2]
            }
            _ => {
                let bright_bit = grid.bright_phase() as usize;
                let power_mode = POWER_MODE.load(Ordering::Relaxed).clamp(1, 2);
                BRIGHT_TIMES[bright_bit][(power_mode - 1) as usize]
            }
        };

        pac::TIM3
            .cnt()
            .write_value(pac::timer::regs::CntCore(next_delay as u32));
        SURFACE_MODE.store((mode + 1) & 3, Ordering::Relaxed);
    } else {
        // The bright phase is pending but the previous SPI2/DMA shift-out
        // hasn't completed yet: re-arm a very short poll interval instead of
        // unblanking with stale shift-register contents.
        pac::TIM3
            .cnt()
            .write_value(pac::timer::regs::CntCore(DMA_POLL_TICKS as u32));
    }
}

/// DMA1 Channel 4 (SPI2 RX) transfer-complete interrupt. In a full-duplex
/// SPI transfer the RX side always completes last, making this the
/// authoritative "shift is fully done" signal for both the outgoing LED
/// data and the incoming switch data.
#[cortex_m_rt::interrupt]
fn DMA1_CHANNEL4() {
    pac::DMA1.ch(3).cr().modify(|w| w.set_en(false));
    pac::DMA1.ifcr().write(|w| {
        w.set_gif(3, true);
        w.set_tcif(3, true);
        w.set_htif(3, true);
        w.set_teif(3, true);
    });
    let _ = pac::DMA1.isr().read();
    DMA_FINISHED.store(true, Ordering::Release);
}

/// `BASEPRI` threshold that masks preemption priorities `P1`..`P15` while
/// leaving `P0` unmasked. On this STM32F103 (4 implemented priority bits)
/// embassy encodes `Priority::P1` as `0x10`.
const RESERVE_P0_BASEPRI: u8 = interrupt::Priority::P1 as u8;

/// BASEPRI-based `critical-section` implementation that reserves NVIC
/// preemption priority 0 for the LED scan.
///
/// The stock `critical-section-single-core` impl masks *all* interrupts via
/// `PRIMASK`. embassy takes critical sections several times per millisecond
/// (time driver, `Mutex`), each of which would then also block our P0 `TIM3`
/// scan handler. The bright phase of every subframe is ended by the very next
/// `TIM3` update interrupt, so a delayed handler stretches that subframe's
/// on-time — a large relative error on the short low-order bit-planes (the
/// shortest pulse is ~2 ticks ≈ 1.7 µs), which shows up as flicker/scanlines on
/// dark colors like `(2, 2, 2)`. The reference firmware has no critical
/// sections, so its scan handler is never delayed; that determinism is what we
/// reproduce here.
///
/// Instead of `PRIMASK` we raise `BASEPRI` to priority level 1, masking
/// `P1`..`P15` while leaving `P0` (`TIM3` and the LED DMA-RX handler
/// `DMA1_CHANNEL4`, see [`start_scan`]) always enabled.
///
/// Soundness relies on nothing at `P0` touching state a critical section
/// protects: the only P0 handlers are `TIM3` and `DMA1_CHANNEL4`, which
/// communicate solely through the atomics and torn-read-tolerant LED buffer in
/// this module and never touch embassy-internal state. Every other active
/// interrupt therefore runs at `>= P1` (`USB` at P2; embassy's `TIM4` time
/// driver is bumped from its P0 reset default to P1 in `main`).
struct ScanPriorityCriticalSection;
critical_section::set_impl!(ScanPriorityCriticalSection);

unsafe impl critical_section::Impl for ScanPriorityCriticalSection {
    unsafe fn acquire() -> critical_section::RawRestoreState {
        let previous = basepri::read();
        // `basepri_max` only ever raises the masking threshold, so nested
        // critical sections can never loosen an outer one.
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
