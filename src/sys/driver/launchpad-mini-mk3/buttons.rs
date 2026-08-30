// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

const LP_BUTTON_COUNT: usize = 100;
const LP_SCAN_GROUPS: usize = 4;
const PAD_SCAN_MAP: [u8; LP_BUTTON_COUNT] = [
    0xff, 0x00, 0x40, 0x80, 0xc0, 0x01, 0x41, 0x81, 0xc1, 0xff, 0xff, 0x02, 0x42, 0x82, 0xc2, 0x03,
    0x43, 0x83, 0xc3, 0x12, 0xff, 0x04, 0x44, 0x84, 0xc4, 0x05, 0x45, 0x85, 0xc5, 0x52, 0xff, 0x06,
    0x46, 0x86, 0xc6, 0x07, 0x47, 0x87, 0xc7, 0x92, 0xff, 0x08, 0x48, 0x88, 0xc8, 0x09, 0x49, 0x89,
    0xc9, 0xd2, 0xff, 0x0a, 0x4a, 0x8a, 0xca, 0x0b, 0x4b, 0x8b, 0xcb, 0x13, 0xff, 0x0c, 0x4c, 0x8c,
    0xcc, 0x0d, 0x4d, 0x8d, 0xcd, 0x53, 0xff, 0x0e, 0x4e, 0x8e, 0xce, 0x0f, 0x4f, 0x8f, 0xcf, 0x93,
    0xff, 0x10, 0x50, 0x90, 0xd0, 0x11, 0x51, 0x91, 0xd1, 0xd3, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff,
];

pub struct Buttons {
    btn_hist: [[u8; 32]; LP_SCAN_GROUPS],
}

impl Buttons {
    pub fn new() -> Self {
        Self {
            btn_hist: [[0; 32]; LP_SCAN_GROUPS],
        }
    }

    pub fn capture_scan(&mut self, group: u8, row: u8, data: &[u8]) {
        if group as usize >= LP_SCAN_GROUPS || row >= 4 || data.len() < 8 {
            return;
        }

        let start: usize = row as usize * 8;
        self.btn_hist[group as usize][start..start + 8].copy_from_slice(&data[..8]);
    }

    pub fn is_valid(&self, index: u8) -> bool {
        let index = index as usize;
        index < LP_BUTTON_COUNT && PAD_SCAN_MAP[index] != 0xff
    }

    pub fn is_pressed(&self, index: u8) -> bool {
        self.strength(index) >= 2
    }

    pub fn strength(&self, index: u8) -> u8 {
        if !self.is_valid(index) {
            return 0;
        }

        let map: u8 = PAD_SCAN_MAP[index as usize];
        let byte_index: usize = (map >> 3) as usize;
        let mask: u8 = 1 << (map & 7);

        let mut sum: u8 = 0;
        for group in 0..LP_SCAN_GROUPS {
            if self.btn_hist[group][byte_index] & mask != 0 {
                sum += 1;
            }
        }

        return sum;
    }
}
