// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::palette_editor::{BACK_BUTTON, hsv_to_rgb, scale_color};
use crate::sys::{led, settings};

const VALUE_MIN: u8 = 4;

pub enum Action {
    None,
    Back,
    Select { saturation: u8, lightness: u8 },
}

pub fn render(hue: u8, factor: u8) {
    for row in 0..8 {
        let lightness = row_to_lightness6(row) * 4;

        for col in 0..8 {
            let saturation = col_to_saturation6(col) * 4;
            let pad = (row + 1) * 10 + (col + 1);

            led::set(
                pad,
                scale_color(hsv_to_rgb(hue, saturation, lightness), factor),
            );
        }
    }

    led::set(BACK_BUTTON, scale_color(0xff2000, factor));
}

pub fn handle_press(index: u8) -> Action {
    if index == BACK_BUTTON {
        return Action::Back;
    }

    let row = index / 10;
    let col = index % 10;

    if !(1..=8).contains(&row) || !(1..=8).contains(&col) {
        return Action::None;
    }

    Action::Select {
        saturation: col_to_saturation6(col - 1) * 4,
        lightness: row_to_lightness6(row - 1) * 4,
    }
}

pub fn store(index: u8, hue: u8, saturation: u8, lightness: u8) {
    let color = hsv_to_rgb(hue, saturation, lightness);

    settings::update(|settings| {
        let palette = settings.palette.saturating_sub(4).min(2) as usize;
        let index = index as usize;

        settings.custom_palette[palette][0][index] = ((color >> 16) & 0xff) as u8 / 4;
        settings.custom_palette[palette][1][index] = ((color >> 8) & 0xff) as u8 / 4;
        settings.custom_palette[palette][2][index] = (color & 0xff) as u8 / 4;
    });
}

fn col_to_saturation6(col: u8) -> u8 {
    ((col as u16 * 63) / 7) as u8
}

fn row_to_lightness6(row: u8) -> u8 {
    VALUE_MIN + ((row as u16 * (63 - VALUE_MIN) as u16) / 7) as u8
}
