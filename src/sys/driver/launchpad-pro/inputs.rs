// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

//! Pressure-sensitive pad + side-button input pipeline.
//!
//! This is a clean-room reimplementation of the reference Launchpad Pro
//! firmware's `pad_calculate` (200 Hz pad state machine) and
//! `surface_checkswitches` (1 kHz digital switch debounce) routines, reverse
//! engineered from the original firmware disassembly. The pad state machine
//! in particular replicates the exact hysteresis/deadband/arm-confirm
//! sequence used to prevent double-triggering on pressure-sensitive pads.

use heapless::spsc::{Consumer, Producer, Queue};
use static_cell::StaticCell;

pub const PAD_COUNT: usize = 64;
pub const SIDE_BUTTON_COUNT: usize = 32;
pub const SWITCH_ENTRY_COUNT: usize = SIDE_BUTTON_COUNT + 1;
pub const SETUP_INDEX: u8 = 0;
pub const NO_BUTTON: u8 = 0xff;
pub const PRESS_VALUE: u8 = 127;

const EVENT_QUEUE_SIZE: usize = 64;
const SWITCH_RELEASE_DELAY: u8 = 0x28;
const ADC_BANK_SIZE: usize = 16;

// --- Pad calibration constants (from PadCalibration / pad_setcalibration) ---
// Reference: pad_setcalibration(0x100, 0x3ff, 0x200, 0x301)
const CAL_FLOOR_IDLE: u16 = 0x100; // 256 - idle press threshold (10-bit ADC domain)
const CAL_FLOOR_ACTIVE: u16 = 0xf6; // max(4, 256-10) = 246 - hysteresis floor once non-idle
const CAL_DEADBAND: u16 = 0x50; // 80 - must exceed floor by this much before any velocity math runs
const CAL_AT_START: u16 = 0x200; // 512
const CAL_AT_END: u16 = 0x301; // 769
const CAL_AT_RANGE: u16 = CAL_AT_END - CAL_AT_START; // 257

// Velocity curve index scaling: idx = clamp(400*overshoot / (floor_idle+320), 0, 511)
const VELOCITY_SCALE_NUM: u32 = 0x190; // 400
const VELOCITY_SCALE_DEN: u32 = CAL_FLOOR_IDLE as u32 + 0x140; // 576
const VELOCITY_INDEX_MAX: u32 = 0x1ff; // 511

const FAST_FIRE_VELOCITY: u8 = 0x7e; // vel > 126 (i.e. == 127) fires immediately, bypassing arm-confirm
const ARM_CONFIRM_TICKS: u8 = 1; // 200Hz ticks required before a non-saturated hit fires a note-on
const AT_HOLDOFF_TICKS: u8 = 0x10; // 16 ticks (80ms) aftertouch throttle after note-on
const RELEASE_ARM_TICKS: u8 = 4; // 20ms below-floor before note-off fires

/// Selects which of the two 512-byte velocity curve LUTs is used, or a fixed
/// velocity (127) mode. Only curve 1 (the device default) is embedded.
const CURRENT_CURVE_FIXED: bool = false;

/// Per-pad velocity state machine states.
const PAD_STATE_IDLE: u8 = 0;
const PAD_STATE_ARMING: u8 = 1;
const PAD_STATE_HELD: u8 = 2;
const PAD_STATE_RELEASING: u8 = 3;

/// Default (curve 1) pad velocity lookup table, 512 entries mapping a
/// normalized overshoot index (0..511) to a MIDI velocity (0..127).
/// Extracted byte-for-byte from the reference firmware's PADVELOCITY table.
#[rustfmt::skip]
const PADVELOCITY: [u8; 512] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
    9, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11,
    11, 11, 11, 11, 11, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 13,
    13, 13, 13, 13, 13, 13, 14, 14, 14, 14, 14, 14, 14, 14, 15, 15,
    15, 15, 15, 15, 15, 16, 16, 16, 16, 16, 16, 17, 17, 17, 17, 17,
    17, 18, 18, 18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 20, 20, 20,
    20, 20, 21, 21, 21, 21, 21, 22, 22, 22, 22, 22, 23, 23, 23, 23,
    23, 24, 24, 24, 24, 25, 25, 25, 25, 26, 26, 26, 26, 27, 27, 27,
    27, 28, 28, 28, 28, 29, 29, 29, 29, 30, 30, 30, 31, 31, 31, 31,
    32, 32, 32, 33, 33, 33, 34, 34, 34, 35, 35, 35, 36, 36, 36, 37,
    37, 37, 38, 38, 38, 39, 39, 40, 40, 40, 41, 41, 41, 42, 42, 43,
    43, 43, 44, 44, 45, 45, 46, 46, 46, 47, 47, 48, 48, 49, 49, 50,
    50, 51, 51, 51, 52, 52, 53, 53, 54, 54, 55, 56, 56, 57, 57, 58,
    58, 59, 59, 60, 60, 61, 62, 62, 63, 63, 64, 65, 65, 66, 66, 67,
    68, 68, 69, 70, 70, 71, 72, 72, 73, 75, 78, 80, 82, 85, 87, 89,
    92, 94, 96, 99, 101, 104, 106, 108, 111, 113, 115, 118, 120, 122, 125, 127,
    127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127,
    127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127,
];

const PADADC: [u8; PAD_COUNT] = [
    49, 51, 53, 55, 57, 59, 61, 63, 33, 35, 37, 39, 41, 43, 45, 47, 17, 19, 21, 23, 25, 27, 29, 31,
    1, 3, 5, 7, 9, 11, 13, 15, 48, 50, 52, 54, 56, 58, 60, 62, 32, 34, 36, 38, 40, 42, 44, 46, 16,
    18, 20, 22, 24, 26, 28, 30, 0, 2, 4, 6, 8, 10, 12, 14,
];

pub const PAD_SENSOR_TO_INDEX: [u8; PAD_COUNT] = [
    81, 82, 83, 84, 85, 86, 87, 88, 71, 72, 73, 74, 75, 76, 77, 78, 61, 62, 63, 64, 65, 66, 67, 68,
    51, 52, 53, 54, 55, 56, 57, 58, 41, 42, 43, 44, 45, 46, 47, 48, 31, 32, 33, 34, 35, 36, 37, 38,
    21, 22, 23, 24, 25, 26, 27, 28, 11, 12, 13, 14, 15, 16, 17, 18,
];

pub const SWITCH_TO_INDEX: [u8; SWITCH_ENTRY_COUNT] = [
    // Group 0
    94,
    98,
    50,
    10,
    SETUP_INDEX,
    4,
    8,
    59,
    19,
    // Group 1
    91,
    95,
    80,
    40,
    1,
    5,
    89,
    49,
    // Group 2
    92,
    96,
    70,
    30,
    2,
    6,
    79,
    39,
    // Group 3
    93,
    97,
    60,
    20,
    3,
    7,
    69,
    29,
];

#[derive(Copy, Clone)]
pub enum GridEvent {
    Press { index: u8, value: u8 },
    Release { index: u8 },
    Aftertouch { index: u8, value: u8 },
}

pub struct Inputs {
    producer: Producer<'static, GridEvent>,
    consumer: Consumer<'static, GridEvent>,
    adc_direct: [u16; PAD_COUNT],
    adc_max: [u16; PAD_COUNT],
    pad_state: [u8; PAD_COUNT],
    pad_time: [u8; PAD_COUNT],
    last_aftertouch: [u8; PAD_COUNT],
    pads_pressed: [bool; PAD_COUNT],
    switches_raw: [u8; SWITCH_ENTRY_COUNT],
    switches_processed: [u8; SWITCH_ENTRY_COUNT],
}

impl Inputs {
    pub fn new() -> Self {
        static QUEUE: StaticCell<Queue<GridEvent, EVENT_QUEUE_SIZE>> = StaticCell::new();
        let (producer, consumer) = QUEUE.init(Queue::new()).split();

        Self {
            producer,
            consumer,
            adc_direct: [0; PAD_COUNT],
            adc_max: [0; PAD_COUNT],
            pad_state: [0; PAD_COUNT],
            pad_time: [0; PAD_COUNT],
            last_aftertouch: [0; PAD_COUNT],
            pads_pressed: [false; PAD_COUNT],
            switches_raw: [0; SWITCH_ENTRY_COUNT],
            switches_processed: [0; SWITCH_ENTRY_COUNT],
        }
    }

    pub fn poll_event(&mut self) -> Option<GridEvent> {
        self.consumer.dequeue()
    }

    pub fn capture_pad_velocity(&mut self, sensor: usize, value: u8) {
        let Some(&index) = PAD_SENSOR_TO_INDEX.get(sensor) else {
            return;
        };

        let pressed = value != 0;
        if self.pads_pressed[sensor] == pressed {
            return;
        }

        self.pads_pressed[sensor] = pressed;
        let event = if pressed {
            GridEvent::Press { index, value }
        } else {
            GridEvent::Release { index }
        };
        let _ = self.producer.enqueue(event);
    }

    pub fn capture_pad_aftertouch(&mut self, sensor: usize, value: u8) {
        let Some(&index) = PAD_SENSOR_TO_INDEX.get(sensor) else {
            return;
        };

        let _ = self
            .producer
            .enqueue(GridEvent::Aftertouch { index, value });
    }

    pub fn capture_switch_entry(&mut self, entry: usize, pressed: bool) {
        let Some(&index) = SWITCH_TO_INDEX.get(entry) else {
            return;
        };

        if index == NO_BUTTON {
            return;
        }

        let state = &mut self.switches_processed[entry];
        let event = if pressed {
            switch_is_on(state).then_some(GridEvent::Press {
                index,
                value: PRESS_VALUE,
            })
        } else {
            switch_is_off(state, SWITCH_RELEASE_DELAY).then_some(GridEvent::Release { index })
        };

        if let Some(event) = event {
            let _ = self.producer.enqueue(event);
        }
    }

    pub fn set_switch_raw(&mut self, entry: usize, value: bool) {
        if let Some(slot) = self.switches_raw.get_mut(entry) {
            *slot = value as u8;
        }
    }

    pub fn capture_adc_bank(&mut self, bank: usize, samples: &[u16; ADC_BANK_SIZE]) {
        let base = bank * ADC_BANK_SIZE;
        self.adc_direct[base..base + ADC_BANK_SIZE].copy_from_slice(samples);
    }

    /// Peak-hold accumulation, called once per full 64-channel ADC sweep
    /// (~1kHz). This never resets `adc_max` itself - only `tick_200hz`'s
    /// state machine clears it, and only on specific transitions, exactly
    /// matching the reference firmware's `adc_runMaxSimple`.
    pub fn accumulate_adc_max(&mut self) {
        for sensor in 0..PAD_COUNT {
            let value = (self.adc_direct[PADADC[sensor] as usize] + 2) >> 2;
            if self.adc_max[sensor] < value {
                self.adc_max[sensor] = value;
            }
        }
    }

    pub fn tick_1khz(&mut self) {
        for entry in 0..SWITCH_ENTRY_COUNT {
            self.capture_switch_entry(entry, self.switches_raw[entry] != 0);
        }
    }

    pub fn tick_200hz(&mut self) {
        for sensor in 0..PAD_COUNT {
            self.process_pad(sensor);
        }
    }

    /// Per-pad 200Hz velocity/aftertouch state machine. This is a
    /// clean-room reimplementation of the reference firmware's
    /// `pad_calculate`, traced instruction-by-instruction from the original
    /// disassembly to exactly reproduce its deadband, hysteresis,
    /// arm-confirm and re-arm-suppression behaviour - the mechanisms that
    /// prevent double-triggering.
    fn process_pad(&mut self, sensor: usize) {
        let adc_max = self.adc_max[sensor];
        let state = self.pad_state[sensor];

        let floor = if state != PAD_STATE_IDLE {
            CAL_FLOOR_ACTIVE
        } else {
            CAL_FLOOR_IDLE
        };

        if adc_max <= floor {
            self.process_below_threshold(sensor, state);
            return;
        }

        let diff = adc_max - floor;
        if diff <= CAL_DEADBAND {
            // Inside the deadband: treated identically to being below the floor.
            self.process_below_threshold(sensor, state);
            return;
        }

        let overshoot = diff - CAL_DEADBAND;
        let velocity = self.velocity_for_overshoot(overshoot);

        match state {
            PAD_STATE_IDLE => {
                if velocity != 0 {
                    if velocity > FAST_FIRE_VELOCITY {
                        // Already-saturated hit: fire immediately, skipping
                        // the arm-confirm delay. NOTE: adc_max is
                        // deliberately NOT cleared here, matching the
                        // reference exactly (verified against disassembly).
                        self.pad_time[sensor] = AT_HOLDOFF_TICKS;
                        self.capture_pad_velocity(sensor, velocity);
                        self.pad_state[sensor] = PAD_STATE_HELD;
                    } else {
                        self.pad_state[sensor] = PAD_STATE_ARMING;
                        self.pad_time[sensor] = ARM_CONFIRM_TICKS;
                    }
                }
            }
            PAD_STATE_ARMING => {
                if velocity <= FAST_FIRE_VELOCITY {
                    self.pad_time[sensor] = self.pad_time[sensor].saturating_sub(1);
                    if self.pad_time[sensor] == 0 {
                        self.pad_time[sensor] = AT_HOLDOFF_TICKS;
                        self.capture_pad_velocity(sensor, velocity);
                        self.pad_state[sensor] = PAD_STATE_HELD;
                        self.adc_max[sensor] = 0;
                    }
                } else {
                    // Saturated while arming: fire immediately without
                    // waiting for the arm-confirm counter to expire.
                    self.pad_time[sensor] = AT_HOLDOFF_TICKS;
                    self.capture_pad_velocity(sensor, velocity);
                    self.pad_state[sensor] = PAD_STATE_HELD;
                    self.adc_max[sensor] = 0;
                }
            }
            PAD_STATE_HELD => {
                if velocity == 0 {
                    self.pad_time[sensor] = RELEASE_ARM_TICKS;
                    self.pad_state[sensor] = PAD_STATE_RELEASING;
                    self.adc_max[sensor] = 0;
                } else if self.pad_time[sensor] != 0 {
                    // Still throttling aftertouch after a recent note-on.
                    self.pad_time[sensor] -= 1;
                    self.adc_max[sensor] = 0;
                } else {
                    let at = aftertouch_from_diff(diff);
                    if self.last_aftertouch[sensor] != at {
                        self.last_aftertouch[sensor] = at;
                        self.capture_pad_aftertouch(sensor, at);
                    }
                    self.adc_max[sensor] = 0;
                }
            }
            PAD_STATE_RELEASING => {
                // A re-press during the release-holdoff window is absorbed:
                // go straight back to "held", never back through idle/arm.
                // This is the key anti-double-trigger mechanism.
                self.pad_time[sensor] = 0;
                self.pad_state[sensor] = PAD_STATE_HELD;
                self.adc_max[sensor] = 0;
            }
            _ => {
                self.pad_state[sensor] = PAD_STATE_IDLE;
            }
        }
    }

    fn process_below_threshold(&mut self, sensor: usize, state: u8) {
        match state {
            PAD_STATE_IDLE => {
                // Nothing to do; adc_max is intentionally left as-is
                // (harmless peak-hold residue below the idle floor).
            }
            PAD_STATE_ARMING => {
                self.pad_time[sensor] = self.pad_time[sensor].saturating_sub(1);
                if self.pad_time[sensor] == 0 {
                    self.pad_time[sensor] = AT_HOLDOFF_TICKS;
                    self.capture_pad_velocity(sensor, 0);
                    self.pad_state[sensor] = PAD_STATE_HELD;
                    self.adc_max[sensor] = 0;
                }
                // else: stay arming, no event yet.
            }
            PAD_STATE_HELD => {
                self.pad_time[sensor] = RELEASE_ARM_TICKS;
                self.pad_state[sensor] = PAD_STATE_RELEASING;
                self.adc_max[sensor] = 0;
            }
            PAD_STATE_RELEASING => {
                self.pad_time[sensor] = self.pad_time[sensor].saturating_sub(1);
                if self.pad_time[sensor] == 0 {
                    if self.last_aftertouch[sensor] != 0 {
                        self.last_aftertouch[sensor] = 0;
                        self.capture_pad_aftertouch(sensor, 0);
                    }
                    self.capture_pad_velocity(sensor, 0);
                    self.pad_state[sensor] = PAD_STATE_IDLE;
                }
                self.adc_max[sensor] = 0;
            }
            _ => {
                self.pad_state[sensor] = PAD_STATE_IDLE;
            }
        }
    }

    fn velocity_for_overshoot(&self, overshoot: u16) -> u8 {
        if CURRENT_CURVE_FIXED {
            return 0x7f;
        }

        let idx = (VELOCITY_SCALE_NUM * overshoot as u32 / VELOCITY_SCALE_DEN)
            .min(VELOCITY_INDEX_MAX) as usize;
        PADVELOCITY[idx]
    }
}

fn aftertouch_from_diff(diff: u16) -> u8 {
    if diff <= CAL_AT_START {
        return 0;
    }

    let value = (diff - CAL_AT_START).min(CAL_AT_RANGE);
    ((value as u32 * 127) / CAL_AT_RANGE as u32) as u8
}

fn switch_is_on(state: &mut u8) -> bool {
    let current = *state;
    if current == 0 {
        *state = 2;
        return false;
    }

    if current < 0x80 {
        let next = current - 1;
        if next == 0 {
            *state = 0x80;
            return true;
        }
        *state = next;
        return false;
    }

    if current != 0x80 {
        *state = 0x80;
    }

    false
}

fn switch_is_off(state: &mut u8, release_delay: u8) -> bool {
    let current = *state;
    if current == 0 {
        return false;
    }

    if current < 0x80 {
        *state = 0;
        return false;
    }

    if current == 0x80 {
        *state = release_delay.wrapping_add(0x80);
        return false;
    }

    let next = current.wrapping_sub(1);
    if next == 0x80 {
        *state = 0;
        return true;
    }

    *state = next;
    false
}
