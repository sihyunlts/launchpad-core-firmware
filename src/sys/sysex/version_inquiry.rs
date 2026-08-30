// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::AppId;
use crate::sys::midi::MidiPort;

pub const CAP_PALETTE: u16 = 1 << 0;
pub const CAP_FASTLED: u16 = 1 << 1;
pub const CAP_M0_STATUS: u16 = 1 << 2;
// Bit 3 was the retired CoreFW M0 flashing path. Keep it reserved so the
// remaining capability bits retain their published wire positions.
pub const CAP_M0_STATS: u16 = 1 << 4;

const CAPABILITY_QUERY: [u8; 9] = [0xf0, 0x00, 0x20, 0x29, 0x02, 0x7f, 0x02, 0x01, 0xf7];

#[cfg(feature = "launchpad-pro-mk3")]
const BUILD_CAPABILITIES: u16 = CAP_PALETTE | CAP_FASTLED | CAP_M0_STATUS | CAP_M0_STATS;

#[cfg(not(feature = "launchpad-pro-mk3"))]
const BUILD_CAPABILITIES: u16 = CAP_PALETTE | CAP_FASTLED;

const fn parse_u8(s: &str) -> u8 {
    let bytes = s.as_bytes();
    let mut val = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b >= b'0' && b <= b'9' {
            val = val * 10 + (b - b'0');
        }
        i += 1;
    }
    val
}

pub fn execute(_app: AppId, port: MidiPort, data: &[u8]) -> bool {
    if data.starts_with(&[0xf0, 0x00, 0x20, 0x29, 0x02, 0x7f, 0x00]) {
        const MAJOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MAJOR"));
        const MINOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MINOR"));
        const PATCH: u8 = parse_u8(env!("CARGO_PKG_VERSION_PATCH"));

        let device_id = crate::driver::device_id();

        let resp = [
            0xf0,
            0x00,
            0x20,
            0x29,
            0x02,
            0x7f,
            0x01,
            device_id,
            MAJOR & 0x7f,
            MINOR & 0x7f,
            PATCH & 0x7f,
            0xf7,
        ];

        crate::driver::send_midi(port, &resp);
        true
    } else if data == CAPABILITY_QUERY {
        let resp = capability_response(
            crate::driver::device_id(),
            [
                parse_u8(env!("CARGO_PKG_VERSION_MAJOR")),
                parse_u8(env!("CARGO_PKG_VERSION_MINOR")),
                parse_u8(env!("CARGO_PKG_VERSION_PATCH")),
            ],
            BUILD_CAPABILITIES,
        );
        crate::driver::send_midi(port, &resp);
        true
    } else {
        false
    }
}

/// Schema-1 response for `F0 00 20 29 02 7F 02 01 F7`:
///
/// `F0 00 20 29 02 7F 03 01 <device-id> <major> <minor> <patch>
///  <flags-lsb-7bit> <flags-msb-7bit> F7`.
const fn capability_response(device_id: u8, version: [u8; 3], flags: u16) -> [u8; 15] {
    [
        0xf0,
        0x00,
        0x20,
        0x29,
        0x02,
        0x7f,
        0x03,
        0x01,
        device_id & 0x7f,
        version[0] & 0x7f,
        version[1] & 0x7f,
        version[2] & 0x7f,
        (flags as u8) & 0x7f,
        ((flags >> 7) as u8) & 0x7f,
        0xf7,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_response_schema_1_wire_shape() {
        assert_eq!(
            capability_response(0x23, [1, 2, 3], CAP_PALETTE | CAP_FASTLED),
            [
                0xf0, 0x00, 0x20, 0x29, 0x02, 0x7f, 0x03, 0x01, 0x23, 0x01, 0x02, 0x03, 0x03, 0x00,
                0xf7,
            ]
        );
    }

    #[test]
    fn capability_flags_are_7_bit_split() {
        assert_eq!(
            capability_response(0xff, [0xff, 0x80, 0x81], 0x01ff),
            [
                0xf0, 0x00, 0x20, 0x29, 0x02, 0x7f, 0x03, 0x01, 0x7f, 0x7f, 0x00, 0x01, 0x7f, 0x03,
                0xf7,
            ]
        );
    }

    #[cfg(feature = "launchpad-pro-mk3")]
    #[test]
    fn pro_mk3_advertises_all_supported_capabilities() {
        assert_eq!(
            BUILD_CAPABILITIES,
            CAP_PALETTE | CAP_FASTLED | CAP_M0_STATUS | CAP_M0_STATS
        );
    }

    #[cfg(not(feature = "launchpad-pro-mk3"))]
    #[test]
    fn non_pro_mk3_builds_advertise_only_common_capabilities() {
        assert_eq!(BUILD_CAPABILITIES, CAP_PALETTE | CAP_FASTLED);
    }
}
