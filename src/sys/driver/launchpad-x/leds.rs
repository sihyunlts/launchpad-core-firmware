// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister
// Copyright (C) 2026 ZephyrCodesStuff

const LP_LED_COUNT: usize = 100;
const LP_LED_BITS: usize = 256;
const LP_LED_PLANES: usize = 6;

// ARM Cortex-M4 Quad 8-bit SIMD Color Helpers (1 clock cycle per 4 channels)
#[inline(always)]
pub fn blend_rgb_50_50(color_a: u32, color_b: u32) -> u32 {
    let res: u32;
    unsafe {
        core::arch::asm!(
            "uhadd8 {0}, {1}, {2}",
            out(reg) res,
            in(reg) color_a,
            in(reg) color_b,
            options(nomem, nostack, preserves_flags)
        );
    }
    res
}

#[inline(always)]
pub fn add_rgb_saturating(color_a: u32, color_b: u32) -> u32 {
    let res: u32;
    unsafe {
        core::arch::asm!(
            "uqadd8 {0}, {1}, {2}",
            out(reg) res,
            in(reg) color_a,
            in(reg) color_b,
            options(nomem, nostack, preserves_flags)
        );
    }
    res
}

const OVERLAY_INDEX: u16 = 0x270e;

const GREEN_MAP: [u16; LP_LED_COUNT] = [
    0x270f, 0x0035, 0x0075, 0x00b5, 0x00f5, 0x0032, 0x0072, 0x00b2, 0x00f2, 0x270e, 0x270f, 0x002f,
    0x006f, 0x00af, 0x00ef, 0x002c, 0x006c, 0x00ac, 0x00ec, 0x003d, 0x270f, 0x0029, 0x0069, 0x00a9,
    0x00e9, 0x0026, 0x0066, 0x00a6, 0x00e6, 0x007d, 0x270f, 0x0023, 0x0063, 0x00a3, 0x00e3, 0x0020,
    0x0060, 0x00a0, 0x00e0, 0x00bd, 0x270f, 0x001d, 0x005d, 0x009d, 0x00dd, 0x001a, 0x005a, 0x009a,
    0x00da, 0x00fd, 0x270f, 0x0017, 0x0057, 0x0097, 0x00d7, 0x0014, 0x0054, 0x0094, 0x00d4, 0x003a,
    0x270f, 0x0011, 0x0051, 0x0091, 0x00d1, 0x000e, 0x004e, 0x008e, 0x00ce, 0x007a, 0x270f, 0x000b,
    0x004b, 0x008b, 0x00cb, 0x0008, 0x0048, 0x0088, 0x00c8, 0x00ba, 0x270f, 0x0005, 0x0045, 0x0085,
    0x00c5, 0x0002, 0x0042, 0x0082, 0x00c2, 0x00fa, 0x270f, 0x270f, 0x270f, 0x270f, 0x270f, 0x270f,
    0x270f, 0x270f, 0x270f, 0x270f,
];
const RED_MAP: [u16; LP_LED_COUNT] = [
    0x270f, 0x0036, 0x0076, 0x00b6, 0x00f6, 0x0033, 0x0073, 0x00b3, 0x00f3, 0x270e, 0x270f, 0x0030,
    0x0070, 0x00b0, 0x00f0, 0x002d, 0x006d, 0x00ad, 0x00ed, 0x003e, 0x270f, 0x002a, 0x006a, 0x00aa,
    0x00ea, 0x0027, 0x0067, 0x00a7, 0x00e7, 0x007e, 0x270f, 0x0024, 0x0064, 0x00a4, 0x00e4, 0x0021,
    0x0061, 0x00a1, 0x00e1, 0x00be, 0x270f, 0x001e, 0x005e, 0x009e, 0x00de, 0x001b, 0x005b, 0x009b,
    0x00db, 0x00fe, 0x270f, 0x0018, 0x0058, 0x0098, 0x00d8, 0x0015, 0x0055, 0x0095, 0x00d5, 0x003b,
    0x270f, 0x0012, 0x0052, 0x0092, 0x00d2, 0x000f, 0x004f, 0x008f, 0x00cf, 0x007b, 0x270f, 0x000c,
    0x004c, 0x008c, 0x00cc, 0x0009, 0x0049, 0x0089, 0x00c9, 0x00bb, 0x270f, 0x0006, 0x0046, 0x0086,
    0x00c6, 0x0003, 0x0043, 0x0083, 0x00c3, 0x00fb, 0x270f, 0x270f, 0x270f, 0x270f, 0x270f, 0x270f,
    0x270f, 0x270f, 0x270f, 0x270f,
];
const BLUE_MAP: [u16; LP_LED_COUNT] = [
    0x270f, 0x0037, 0x0077, 0x00b7, 0x00f7, 0x0034, 0x0074, 0x00b4, 0x00f4, 0x270e, 0x270f, 0x0031,
    0x0071, 0x00b1, 0x00f1, 0x002e, 0x006e, 0x00ae, 0x00ee, 0x003f, 0x270f, 0x002b, 0x006b, 0x00ab,
    0x00eb, 0x0028, 0x0068, 0x00a8, 0x00e8, 0x007f, 0x270f, 0x0025, 0x0065, 0x00a5, 0x00e5, 0x0022,
    0x0062, 0x00a2, 0x00e2, 0x00bf, 0x270f, 0x001f, 0x005f, 0x009f, 0x00df, 0x001c, 0x005c, 0x009c,
    0x00dc, 0x00ff, 0x270f, 0x0019, 0x0059, 0x0099, 0x00d9, 0x0016, 0x0056, 0x0096, 0x00d6, 0x003c,
    0x270f, 0x0013, 0x0053, 0x0093, 0x00d3, 0x0010, 0x0050, 0x0090, 0x00d0, 0x007c, 0x270f, 0x000d,
    0x004d, 0x008d, 0x00cd, 0x000a, 0x004a, 0x008a, 0x00ca, 0x00bc, 0x270f, 0x0007, 0x0047, 0x0087,
    0x00c7, 0x0004, 0x0044, 0x0084, 0x00c4, 0x00fc, 0x270f, 0x270f, 0x270f, 0x270f, 0x270f, 0x270f,
    0x270f, 0x270f, 0x270f, 0x270f,
];

pub struct Leds {
    pub fb: [[u8; 32]; 6],
    pub overlay_r: u8,
    pub overlay_g: u8,
    pub overlay_b: u8,
    brightness: u8,
}

impl Leds {
    pub fn new() -> Self {
        Self {
            fb: [[0xff; 32]; 6],
            overlay_r: 0,
            overlay_g: 0,
            overlay_b: 0,
            brightness: 8,
        }
    }

    pub fn set_led(&mut self, led: u8, rgb: u32) {
        if led as usize >= LP_LED_COUNT {
            return;
        }

        let rgb = rgb & 0x00ff_ffff;
        let r_idx = RED_MAP[led as usize];
        let g_idx = GREEN_MAP[led as usize];
        let b_idx = BLUE_MAP[led as usize];

        self.fb_write_intensity8(r_idx, (rgb >> 16) as u8);
        self.fb_write_intensity8(g_idx, (rgb >> 8) as u8);
        self.fb_write_intensity8(b_idx, rgb as u8);
        self.set_overlay_if_needed(
            r_idx,
            (rgb >> 18) as u8,
            (rgb >> 10) as u8,
            (rgb >> 2) as u8,
        );
    }

    pub fn set_led_rgb(&mut self, led: u8, r: u8, g: u8, b: u8) {
        if led as usize >= LP_LED_COUNT {
            return;
        }

        let r_idx = RED_MAP[led as usize];
        let g_idx = GREEN_MAP[led as usize];
        let b_idx = BLUE_MAP[led as usize];

        self.fb_write_intensity6(r_idx, r);
        self.fb_write_intensity6(g_idx, g);
        self.fb_write_intensity6(b_idx, b);
        self.set_overlay_if_needed(r_idx, r, g, b);
    }

    fn set_overlay_if_needed(&mut self, r_idx: u16, r: u8, g: u8, b: u8) {
        if r_idx == OVERLAY_INDEX {
            self.overlay_r = r & 0x3f;
            self.overlay_g = g & 0x3f;
            self.overlay_b = b & 0x3f;
        }
    }

    fn fb_write_intensity8(&mut self, bit_index: u16, intensity: u8) {
        if bit_index as usize >= LP_LED_BITS {
            return;
        }

        let byte_index = (bit_index as usize) >> 3;
        let mask = 0x80u8 >> ((bit_index as usize) & 7);
        let nmask = !mask;

        for plane in 0..LP_LED_PLANES {
            let dst = &mut self.fb[plane][byte_index];
            let level_bit = (intensity >> (plane + 2)) & 0x01;

            if level_bit == 0 {
                *dst |= mask;
            } else {
                *dst &= nmask;
            }
        }
    }

    fn fb_write_intensity6(&mut self, bit_index: u16, intensity: u8) {
        if bit_index as usize >= LP_LED_BITS {
            return;
        }

        let intensity = intensity & 0x3f;
        let byte_index = (bit_index as usize) >> 3;
        let mask = 0x80u8 >> ((bit_index as usize) & 7);
        let nmask = !mask;

        for plane in 0..LP_LED_PLANES {
            let dst = &mut self.fb[plane][byte_index];
            let level_bit = (intensity >> plane) & 0x01;

            if level_bit == 0 {
                *dst |= mask;
            } else {
                *dst &= nmask;
            }
        }
    }

    pub fn fill(&mut self, rgb: u32) {
        for i in 0..LP_LED_COUNT {
            self.set_led(i as u8, rgb);
        }
    }

    pub fn brightness(&self) -> u8 {
        self.brightness
    }

    pub fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness.min(8);
    }
}
