// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::{
    app::AppId,
    sys::{led, midi::MidiPort},
};

pub fn execute(_app: AppId, _port: MidiPort, data: &[u8]) -> bool {
    handle(data, |index, r, g, b| led::set_rgb(index, r, g, b))
}

pub fn handle(data: &[u8], mut set_led: impl FnMut(u8, u8, u8, u8)) -> bool {
    handle_targets(data, |target, r, g, b| {
        set_target(target, r, g, b, &mut set_led)
    })
}

pub fn handle_targets(mut data: &[u8], mut set_target: impl FnMut(u8, u8, u8, u8)) -> bool {
    if data.len() < 3 || data[0] != 0xf0 || data[1] != 0x5f || data.last() != Some(&0xf7) {
        return false;
    }

    data = &data[2..data.len() - 1];

    while !data.is_empty() {
        if data.len() < 3 {
            break;
        }

        let r = data[0];
        let g = data[1];
        let b = data[2];
        data = &data[3..];

        let mut count = ((r & 0x40) >> 4) | ((g & 0x40) >> 5) | ((b & 0x40) >> 6);
        if count == 0 {
            let Some((&explicit_count, rest)) = data.split_first() else {
                break;
            };
            count = explicit_count;
            data = rest;
        }

        let r = r & 0x3f;
        let g = g & 0x3f;
        let b = b & 0x3f;

        for _ in 0..count {
            let Some((&target, rest)) = data.split_first() else {
                return true;
            };
            data = rest;
            set_target(target, r, g, b);
        }
    }

    true
}

fn set_target(target: u8, r: u8, g: u8, b: u8, set_led: &mut impl FnMut(u8, u8, u8, u8)) {
    match target {
        0 => {
            for index in 0..99 {
                set_led(index, r, g, b);
            }
        }
        #[cfg(feature = "launchpad-pro-mk3")]
        1..=8 => {
            crate::driver::set_rgb_led(100 + target, r, g, b);
            set_led(target, r, g, b);
        }
        #[cfg(feature = "launchpad-pro-mk3")]
        9..=99 => set_led(target, r, g, b),
        #[cfg(not(feature = "launchpad-pro-mk3"))]
        1..=99 => set_led(target, r, g, b),
        100..=109 => {
            let start = (target - 100) * 10 + 1;
            for index in start..start + 8 {
                set_led(index, r, g, b);
            }
        }
        110..=119 => {
            let start = target - 100;
            for index in (start..90).step_by(10) {
                set_led(index, r, g, b);
            }
        }
        _ => {}
    }
}
