// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use super::LED_COUNT;
use super::super::map::LED_REMAP;
use super::super::runtime::M0Link;
use embassy_time::{Duration, Instant};

const LED_FRAME_BYTES: usize = 0x108;
const LED_PLANES: usize = 6;
const LEDS_PER_ROW: u8 = 27;
const LED_ROW_STRIDE: usize = 0x42;
const LED_PLANE_STRIDE: usize = 0x0B;
const LED_FRAME_PERIOD: Duration = Duration::from_micros(2_083);

pub struct Leds {
    fb: [u8; LED_FRAME_BYTES],
    rgb: [u32; LED_COUNT],
    dirty: bool,
    next_tx: Option<Instant>,
}

impl Leds {
    pub fn new() -> Self {
        Self {
            fb: [0xff; LED_FRAME_BYTES],
            rgb: [0; LED_COUNT],
            dirty: true,
            next_tx: None,
        }
    }

    pub fn task(&mut self, link: &mut M0Link) {
        if !link.is_ready() {
            self.next_tx = None;
            return;
        }
        if !self.dirty {
            return;
        }
        if link.led_tx_slots == 0 {
            return;
        }
        let now = Instant::now();
        let next_tx = self.next_tx.get_or_insert(now);
        if now < *next_tx {
            return;
        }

        // Rebuild the framebuffer from our rgb state only when about to send a frame
        self.fb.fill(0xff);
        for mapped in 0..LED_COUNT {
            let color = self.rgb[mapped];
            if color != 0 {
                let r = ((color >> 12) & 0x3f) as u8;
                let g = ((color >> 6) & 0x3f) as u8;
                let b = (color & 0x3f) as u8;
                write_fb(&mut self.fb, mapped as u8, r, g, b);
            }
        }

        if link.send_led_frame(&self.fb) {
            self.dirty = false;
            *next_tx += LED_FRAME_PERIOD;
            if now.saturating_duration_since(*next_tx) >= LED_FRAME_PERIOD {
                *next_tx = now + LED_FRAME_PERIOD;
            }
        }
    }

    pub fn set_rgb_led(&mut self, index: u8, r: u8, g: u8, b: u8) {
        let Some(&mapped) = LED_REMAP.get(index as usize) else {
            return;
        };
        if mapped == 0xff || mapped as usize >= LED_COUNT {
            return;
        }

        let color = pack_rgb(r, g, b);
        let mapped = mapped as usize;
        if self.rgb[mapped] == color {
            return;
        }

        self.rgb[mapped] = color;
        self.dirty = true;
    }

    pub fn set_led(&mut self, index: u8, color: u32) {
        self.set_rgb_led(
            index,
            ((color >> 18) & 0x3f) as u8,
            ((color >> 10) & 0x3f) as u8,
            ((color >> 2) & 0x3f) as u8,
        );
    }
}

fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((scale_channel(r) as u32) << 12) | ((scale_channel(g) as u32) << 6) | scale_channel(b) as u32
}

fn write_fb(fb: &mut [u8; LED_FRAME_BYTES], led: u8, r: u8, g: u8, b: u8) {
    let row = (led / LEDS_PER_ROW) as usize;
    let led_in_row = led % LEDS_PER_ROW;

    let bit_r = led_in_row * 3;
    let bit_g = bit_r + 1;
    let bit_b = bit_r + 2;
    let byte_r = (bit_r >> 3) as usize;
    let byte_g = (bit_g >> 3) as usize;
    let byte_b = (bit_b >> 3) as usize;
    let mask_r = 1u8 << (bit_r & 7);
    let mask_g = 1u8 << (bit_g & 7);
    let mask_b = 1u8 << (bit_b & 7);
    let mut value_r = scale_channel(r);
    let mut value_g = scale_channel(g);
    let mut value_b = scale_channel(b);

    for plane in 0..LED_PLANES {
        let plane_base = (row * LED_ROW_STRIDE) + (plane * LED_PLANE_STRIDE);
        write_plane_bit(fb, plane_base + byte_r, mask_r, value_r & 1);
        write_plane_bit(fb, plane_base + byte_g, mask_g, value_g & 1);
        write_plane_bit(fb, plane_base + byte_b, mask_b, value_b & 1);
        value_r >>= 1;
        value_g >>= 1;
        value_b >>= 1;
    }
}

fn scale_channel(value: u8) -> u8 {
    value.min(0x3f)
}

fn write_plane_bit(fb: &mut [u8; LED_FRAME_BYTES], index: usize, mask: u8, value: u8) {
    if value != 0 {
        fb[index] &= !mask;
    }
}
