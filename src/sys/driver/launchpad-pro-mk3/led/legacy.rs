// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use heapless::Vec;

pub const LED_COLOR_OFF: u32 = 0;
pub const LED_BATCH_SIZE: usize = 15;
pub const LED_HW_UNKNOWN: u32 = 0xffff_ffff;
pub const LED_COMMIT_INDEX: u8 = 0xff;

use super::LED_COUNT;
use super::super::map::LED_REMAP;
use super::super::runtime::M0Link;

pub struct Leds {
    link_seen: bool,
    resync_pending: bool,
    target: [u32; LED_COUNT],
    hw: [u32; LED_COUNT],
    seq: [u64; LED_COUNT],
    global_seq: u64,
    dirty: [bool; LED_COUNT],
}

impl Leds {
    pub fn new() -> Self {
        Self {
            link_seen: false,
            resync_pending: true,
            target: [LED_COLOR_OFF; LED_COUNT],
            hw: [LED_HW_UNKNOWN; LED_COUNT],
            seq: [0; LED_COUNT],
            global_seq: 0,
            dirty: [true; LED_COUNT],
        }
    }

    pub fn task(&mut self, link: &mut M0Link) {
        if !link.is_ready() {
            if self.link_seen {
                self.resync_pending = true;
            }
            self.link_seen = false;
            return;
        }

        if !self.link_seen {
            self.link_seen = true;
            if self.resync_pending {
                self.hw.fill(LED_HW_UNKNOWN);
                self.dirty.fill(true);
                self.resync_pending = false;
            }
        }

        if link.led_tx_slots < 2 {
            return;
        }

        let mut batch = 0;
        let mut batch_idx = [0u8; LED_BATCH_SIZE];
        let mut batch_rgb = [LED_COLOR_OFF; LED_BATCH_SIZE];

        let mut dirty_leds = Vec::<usize, LED_COUNT>::new();
        for i in 0..LED_COUNT {
            if self.dirty[i] {
                let _ = dirty_leds.push(i);
            }
        }

        dirty_leds.sort_unstable_by(|&a, &b| self.seq[b].cmp(&self.seq[a]));

        for &idx in dirty_leds.iter().take(LED_BATCH_SIZE) {
            let rgb = self.target[idx];

            if self.hw[idx] != rgb {
                if !link.send_cmd_55(idx as u8, rgb) {
                    self.mark_unsynced();
                    return;
                }
                batch_idx[batch] = idx as u8;
                batch_rgb[batch] = rgb;
                batch += 1;
            } else {
                self.dirty[idx] = false;
            }
        }

        if batch == 0 {
            return;
        }

        if !link.send_cmd_55(LED_COMMIT_INDEX, LED_COLOR_OFF) {
            self.mark_unsynced();
            return;
        }
        if link.led_tx_slots > 0 {
            link.led_tx_slots -= 1;
        }

        for i in 0..batch {
            let idx = batch_idx[i] as usize;
            self.hw[idx] = batch_rgb[i];
            if self.target[idx] == batch_rgb[i] {
                self.dirty[idx] = false;
            }
        }
    }

    pub fn set_rgb_led(&mut self, index: u8, r: u8, g: u8, b: u8) {
        let Some(&m) = LED_REMAP.get(index as usize) else {
            return;
        };

        let mapped = m as usize;

        if mapped == 0xff {
            return;
        }

        let rgb = ((r.min(0x3f) as u32) << 18)
            | ((g.min(0x3f) as u32) << 10)
            | ((b.min(0x3f) as u32) << 2);

        if self.target[mapped] != rgb {
            self.target[mapped] = rgb;
            self.seq[mapped] = self.global_seq;
            self.global_seq = self.global_seq.wrapping_add(1);
            if self.target[mapped] != self.hw[mapped] || self.resync_pending {
                self.dirty[mapped] = true;
            }
        }
    }

    pub fn set_led(&mut self, index: u8, color: u32) {
        self.set_rgb_led(
            index,
            ((color >> 18) & 0x3f) as u8,
            ((color >> 10) & 0x3f) as u8,
            ((color >> 2) & 0x3f) as u8,
        );
    }

    fn mark_unsynced(&mut self) {
        self.resync_pending = true;
        self.link_seen = false;
    }
}
