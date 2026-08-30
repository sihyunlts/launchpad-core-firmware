// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::sys::led;

pub(crate) struct Text {
    line_mask: [u8; 4],
    color_mask: u8,
    primary_color: u32,
    secondary_color: u32,
}

impl Text {
    pub const fn new(
        line_mask: [u8; 4],
        color_mask: u8,
        primary_color: u32,
        secondary_color: u32,
    ) -> Self {
        Self {
            line_mask,
            color_mask,
            primary_color,
            secondary_color,
        }
    }

    #[inline(never)]
    pub fn draw(&self) {
        for (y, &line) in self.line_mask.iter().enumerate() {
            let base_pos = 88 - (y as u8 * 10);
            for x in 0..8u8 {
                if (line & (1 << x)) == 0 {
                    continue;
                }
                let color = if (self.color_mask & (1 << x)) != 0 {
                    self.primary_color
                } else {
                    self.secondary_color
                };
                led::set(base_pos - x, color);
            }
        }
    }
}
