// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

#[cfg(feature = "no-setup-btn")]
use crate::app::palette_editor::SAVE_BUTTON;
use crate::app::palette_editor::{BACK_BUTTON, NEXT_BUTTON, rgb, scale_color};
use crate::sys::{led, settings};

pub enum Action {
    None,
    #[cfg(feature = "no-setup-btn")]
    SaveAndExit,
    PreviousPage,
    NextPage,
    Select(u8),
}

pub fn render(half_page: u8, factor: u8) {
    let base = half_page * 64;

    for i in 0..64 {
        led::set(
            local_to_pad(i),
            scale_color(get_palette_rgb(base + i), factor),
        );
    }

    led::set(
        BACK_BUTTON,
        scale_color(if half_page > 0 { 0x303030 } else { 0x060606 }, factor),
    );

    led::set(
        NEXT_BUTTON,
        scale_color(if half_page < 1 { 0x303030 } else { 0x060606 }, factor),
    );

    #[cfg(feature = "no-setup-btn")]
    led::pulse(SAVE_BUTTON, 0xffff00);
}

pub fn handle_press(index: u8, half_page: u8) -> Action {
    #[cfg(feature = "no-setup-btn")]
    if index == SAVE_BUTTON {
        return Action::SaveAndExit;
    }

    if index == BACK_BUTTON && half_page > 0 {
        return Action::PreviousPage;
    }

    if index == NEXT_BUTTON && half_page < 1 {
        return Action::NextPage;
    }

    let local = pad_to_local(index);
    if local == 0xff {
        return Action::None;
    }

    let selected_index = local + half_page * 64;
    if selected_index == 0 {
        Action::None
    } else {
        Action::Select(selected_index)
    }
}

fn local_to_pad(local: u8) -> u8 {
    if local < 32 {
        return (local / 4 + 1) * 10 + (local % 4 + 1);
    }

    let j = local - 32;
    (j / 4 + 1) * 10 + (j % 4 + 5)
}

fn pad_to_local(pad: u8) -> u8 {
    let row = pad / 10;
    let col = pad % 10;

    if !(1..=8).contains(&row) || !(1..=8).contains(&col) {
        return 0xff;
    }

    let row = row - 1;
    let col = col - 1;

    if col < 4 {
        row * 4 + col
    } else {
        32 + row * 4 + (col - 4)
    }
}

fn get_palette_rgb(index: u8) -> u32 {
    settings::with(|settings| {
        let palette = settings.palette.saturating_sub(4).min(2) as usize;
        let index = index as usize;

        rgb(
            settings.custom_palette[palette][0][index].min(63) * 4,
            settings.custom_palette[palette][1][index].min(63) * 4,
            settings.custom_palette[palette][2][index].min(63) * 4,
        )
    })
}
