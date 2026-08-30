// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::AppId;
use crate::sys::midi::MidiPort;

#[cfg(feature = "launchpad-mini-mk3")]
const FAMILY_CODE: [u8; 2] = [19, 1];

#[cfg(feature = "launchpad-x")]
const FAMILY_CODE: [u8; 2] = [3, 1];

#[cfg(feature = "launchpad-pro-mk3")]
const FAMILY_CODE: [u8; 2] = [35, 1];

#[cfg(feature = "launchpad-mk2")]
const FAMILY_CODE: [u8; 2] = [105, 0];

#[cfg(feature = "launchpad-pro")]
const FAMILY_CODE: [u8; 2] = [81, 0];

#[cfg(feature = "launchpad-s")]
const FAMILY_CODE: [u8; 2] = [0x20, 0x00];

#[cfg(feature = "launchpad-mini-mk1")]
const FAMILY_CODE: [u8; 2] = [0x36, 0x00];

pub fn execute(_app: AppId, _port: MidiPort, _data: &[u8]) -> bool {
    #[cfg(any(
        feature = "launchpad-mini-mk3",
        feature = "launchpad-x",
        feature = "launchpad-pro-mk3",
        feature = "launchpad-mk2",
        feature = "launchpad-pro",
        feature = "launchpad-s",
        feature = "launchpad-mini-mk1"
    ))]
    if matches!(_data, [0xf0, 0x7e, _, 0x06, 0x01, 0xf7]) {
        let [family_lsb, family_msb] = FAMILY_CODE;

        let response = [
            0xf0, 0x7e, 0x00, 0x06, 0x02, 0x00, 0x20, 0x29,
            family_lsb, family_msb,
            0x00, 0x00,
            0x00,
            0x09, 0x09, 0x09,
            0xf7,
        ];

        crate::driver::send_midi(_port, &response);
        return true;
    }

    false
}
