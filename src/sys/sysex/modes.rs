// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::AppId;
use crate::sys::midi::MidiPort;

pub fn execute(_app: AppId, _port: MidiPort, data: &[u8]) -> bool {
    switch_target(data).is_some()
}

pub fn switch_target(data: &[u8]) -> Option<AppId> {
    if data.len() == 9
        && data[1] == 0x00
        && data[2] == 0x20
        && data[3] == 0x29
        && data[4] == 0x02
        && data[7] == 0
    {
        Some(AppId::Performance)
    } else {
        None
    }
}
