// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::palette_editor::{BACK_BUTTON, hsv_to_rgb, scale_color};
use crate::sys::led;

const HUE_MAX: u8 = 252;
const HUE_STEP: u8 = 4;

pub enum Action {
    None,
    Back,
    Select(u8),
}

pub fn render(factor: u8) {
    for i in 0..64 {
        let color = hsv_to_rgb(i * 4, HUE_MAX, HUE_MAX);
        led::set(sel_pos_to_pad(i), scale_color(color, factor));
    }

    led::set(BACK_BUTTON, scale_color(0xff2000, factor));
}

pub fn handle_press(index: u8) -> Action {
    if index == BACK_BUTTON {
        return Action::Back;
    }

    let pos = pad_to_sel_pos(index);

    if pos == 0xff {
        Action::None
    } else {
        Action::Select(pos * HUE_STEP)
    }
}

fn sel_pos_to_pad(pos: u8) -> u8 {
    (pos / 8 + 1) * 10 + (pos % 8 + 1)
}

fn pad_to_sel_pos(pad: u8) -> u8 {
    let row = pad / 10;
    let col = pad % 10;

    if !(1..=8).contains(&row) || !(1..=8).contains(&col) {
        return 0xff;
    }

    (row - 1) * 8 + (col - 1)
}
