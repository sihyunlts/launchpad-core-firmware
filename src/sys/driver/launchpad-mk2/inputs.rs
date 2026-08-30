// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use heapless::spsc::{Consumer, Producer, Queue};
use static_cell::StaticCell;

pub const SHIFT_BYTES_PER_SCAN: usize = 8;
pub const GROUP_COUNT: usize = 4;
pub const RAW_BUTTON_BYTES: usize = GROUP_COUNT * SHIFT_BYTES_PER_SCAN;

pub const MK2_KEY_COUNT: usize = 80;
const BUTTON_MAP_COUNT: usize = 96;
const EVENT_QUEUE_SIZE: usize = 64;
const PRESS_VALUE: u8 = 127;
const PRESS_DEBOUNCE_TICKS: u8 = 2;
const RELEASE_DELAY_TICKS: u8 = 0x20;

pub const BUTTON_TO_SURFACE_INDEX: [u8; MK2_KEY_COUNT] = [
    81, 82, 83, 84, 85, 86, 87, 88, 89, 71, 72, 73, 74, 75, 76, 77, 78, 79, 61, 62, 63, 64, 65, 66,
    67, 68, 69, 51, 52, 53, 54, 55, 56, 57, 58, 59, 41, 42, 43, 44, 45, 46, 47, 48, 49, 31, 32, 33,
    34, 35, 36, 37, 38, 39, 21, 22, 23, 24, 25, 26, 27, 28, 29, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    91, 92, 93, 94, 95, 96, 97, 98,
];

const BUTTON_KEY_MAP: [u8; BUTTON_MAP_COUNT] = [
    0x48, 0x4c, 0x00, 0x04, 0x09, 0x0d, 0x12, 0x16, 0x1b, 0x1f, 0x24, 0x28, 0x2d, 0x31, 0x36, 0x3a,
    0x3f, 0x43, 0x08, 0x2c, 0x50, 0x50, 0x50, 0x50, 0x49, 0x4d, 0x01, 0x05, 0x0a, 0x0e, 0x13, 0x17,
    0x1c, 0x20, 0x25, 0x29, 0x2e, 0x32, 0x37, 0x3b, 0x40, 0x44, 0x11, 0x35, 0x50, 0x50, 0x50, 0x50,
    0x4a, 0x4e, 0x02, 0x06, 0x0b, 0x0f, 0x14, 0x18, 0x1d, 0x21, 0x26, 0x2a, 0x2f, 0x33, 0x38, 0x3c,
    0x41, 0x45, 0x1a, 0x3e, 0x50, 0x50, 0x50, 0x50, 0x4b, 0x4f, 0x03, 0x07, 0x0c, 0x10, 0x15, 0x19,
    0x1e, 0x22, 0x27, 0x2b, 0x30, 0x34, 0x39, 0x3d, 0x42, 0x46, 0x23, 0x47, 0x50, 0x50, 0x50, 0x50,
];

const BUTTON_RAW_INDICES: [usize; 12] = [0, 1, 2, 8, 9, 10, 16, 17, 18, 24, 25, 26];

#[derive(Copy, Clone)]
pub enum GridEvent {
    Press { index: u8, value: u8 },
    Release { index: u8 },
}

pub struct Inputs {
    producer: Producer<'static, GridEvent>,
    consumer: Consumer<'static, GridEvent>,
    button_decode: [u8; RAW_BUTTON_BYTES],
    debounce: [u8; MK2_KEY_COUNT],
}

impl Inputs {
    pub fn new() -> Self {
        static QUEUE: StaticCell<Queue<GridEvent, EVENT_QUEUE_SIZE>> = StaticCell::new();
        let (producer, consumer) = QUEUE.init(Queue::new()).split();

        Self {
            producer,
            consumer,
            button_decode: [0; RAW_BUTTON_BYTES],
            debounce: [0; MK2_KEY_COUNT],
        }
    }

    pub fn poll_event(&mut self) -> Option<GridEvent> {
        self.consumer.dequeue()
    }

    pub fn decode_buttons(&mut self, raw: &[u8; RAW_BUTTON_BYTES]) {
        self.button_decode.copy_from_slice(raw);

        for (chunk, &raw_index) in BUTTON_RAW_INDICES.iter().enumerate() {
            let value = self.button_decode[raw_index];
            for bit in 0..8 {
                let key = BUTTON_KEY_MAP[chunk * 8 + bit] as usize;
                if key >= MK2_KEY_COUNT {
                    continue;
                }

                let mask = 0x80 >> bit;
                if value & mask != 0 {
                    if debounce_pressed(&mut self.debounce[key], PRESS_DEBOUNCE_TICKS) {
                        self.queue_event(key, true);
                    }
                } else if debounce_released(&mut self.debounce[key], RELEASE_DELAY_TICKS) {
                    self.queue_event(key, false);
                }
            }
        }
    }

    fn queue_event(&mut self, key: usize, pressed: bool) {
        if key >= MK2_KEY_COUNT {
            return;
        }

        let index = key as u8;

        let event = if pressed {
            GridEvent::Press {
                index,
                value: PRESS_VALUE,
            }
        } else {
            GridEvent::Release { index }
        };
        let _ = self.producer.enqueue(event);
    }
}

fn debounce_pressed(state: &mut u8, press_ticks: u8) -> bool {
    let current = *state;
    if current == 0 {
        *state = press_ticks;
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

fn debounce_released(state: &mut u8, release_delay: u8) -> bool {
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
