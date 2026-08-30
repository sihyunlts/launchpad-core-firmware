// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::AppId;
use crate::sys::midi::MidiPort;
use crate::sys::settings;

pub fn execute(_app: AppId, port: MidiPort, data: &[u8]) -> bool {
    if data.len() >= 5 && data[0] == 0xf0 && data[1] == 0x52 {
        let palette_index = data[2];
        let write_mode = data[3];
        let color_space = (data[4] % 3) as usize;

        if palette_index >= 3 {
            // Only custom palette slots 0, 1, 2 are supported.
            return true;
        }

        if write_mode != 0 {
            // Write palette
            if data.len() >= 134 {
                settings::update(|settings| {
                    for i in 0..128 {
                        settings.custom_palette[palette_index as usize][color_space][i] =
                            data[5 + i];
                    }
                });
                settings::save();
            }
        } else {
            // Read palette
            let mut resp = [0u8; 134];
            resp[0] = 0xf0;
            resp[1] = 0x52;
            resp[2] = palette_index;
            resp[3] = 1;
            resp[4] = color_space as u8;

            settings::with(|settings| {
                for i in 0..128 {
                    resp[5 + i] =
                        settings.custom_palette[palette_index as usize][color_space][i];
                }
            });

            resp[133] = 0xf7;
            crate::driver::send_midi(port, &resp);
        }
        true
    } else {
        false
    }
}
