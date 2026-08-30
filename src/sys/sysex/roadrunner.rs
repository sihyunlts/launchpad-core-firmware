// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::AppId;
use crate::driver::{self, M0FirmwareStatus, M0ProbeResult, RoadrunnerStats};
use crate::sys::midi::MidiPort;

const NOVATION_HEADER: [u8; 6] = [0xf0, 0x00, 0x20, 0x29, 0x02, 0x0e];
const M0_REQ_CMD: u8 = 0x70;
const M0_RESP_CMD: u8 = 0x71;
const M0_STATUS_RESPONSE_MAX_LEN: usize = 73;
const _: () = assert!(
    crate::sys::driver::common::usb::midi::MIDI_TX_MAX_PACKET_COUNT
        >= M0_STATUS_RESPONSE_MAX_LEN.div_ceil(3)
);

pub const M0_ROM_STATUS_OK: u8 = 0;
pub const M0_ROM_STATUS_RX: u8 = 5;
pub const M0_ROM_STATUS_ARG: u8 = 6;

pub fn execute(_app: AppId, port: MidiPort, data: &[u8]) -> bool {
    if data.len() < 8 || data.last() != Some(&0xf7) {
        return false;
    }
    if !data.starts_with(&NOVATION_HEADER) || data[6] != M0_REQ_CMD {
        return false;
    }

    match data[7] {
        b'S' if data.len() == 9 => handle_status(port),
        b'C' if data.len() == 9 => handle_cached_status(port),
        b'F' if data.len() == 9 => handle_flash_info(port),
        b'T' if data.len() == 9 => handle_roadrunner_stats(port),
        _ => send_simple_response(port, data[7], M0_ROM_STATUS_ARG),
    }
    true
}

fn handle_cached_status(port: MidiPort) {
    match driver::cached_m0_firmware_status() {
        Some(status) => send_status_response(port, b'C', &status),
        None => send_simple_response(port, b'C', M0_ROM_STATUS_ARG),
    }
}

fn handle_status(port: MidiPort) {
    match driver::refresh_m0_firmware_status() {
        Some(status) => send_status_response(port, b'S', &status),
        None => send_simple_response(port, b'S', M0_ROM_STATUS_ARG),
    }
}

fn handle_flash_info(port: MidiPort) {
    match driver::flash_info() {
        Some(info) => send_flash_info_response(port, info.present, &info.jedec_id, info.status1),
        None => send_simple_response(port, b'F', M0_ROM_STATUS_ARG),
    }
}

fn handle_roadrunner_stats(port: MidiPort) {
    match driver::roadrunner_stats() {
        Some(Some(stats)) => send_roadrunner_stats_response(port, stats),
        _ => send_simple_response(port, b'T', M0_ROM_STATUS_RX),
    }
}

fn send_status_response(port: MidiPort, cmd: u8, status: &M0FirmwareStatus) {
    let mut resp = [0u8; 128];
    let mut idx = response_prefix(&mut resp, cmd);
    append_hex8(&mut resp, &mut idx, status.status);
    append_hex8(&mut resp, &mut idx, status.kind);
    append_hex8(&mut resp, &mut idx, status.version_major);
    append_hex8(&mut resp, &mut idx, status.version_minor);
    append_hex8(&mut resp, &mut idx, status.version_patch);
    append_probe(&mut resp, &mut idx, &status.probe);
    send_response(port, &mut resp, idx);
}

fn send_flash_info_response(port: MidiPort, present: bool, jedec_id: &[u8; 3], status1: u8) {
    let mut resp = [0u8; 32];
    let mut idx = response_prefix(&mut resp, b'F');
    append_hex8(&mut resp, &mut idx, M0_ROM_STATUS_OK);
    append_hex8(&mut resp, &mut idx, if present { 1 } else { 0 });
    for byte in jedec_id {
        append_hex8(&mut resp, &mut idx, *byte);
    }
    append_hex8(&mut resp, &mut idx, status1);
    send_response(port, &mut resp, idx);
}

fn send_roadrunner_stats_response(port: MidiPort, stats: RoadrunnerStats) {
    let mut resp = [0u8; 32];
    let mut idx = response_prefix(&mut resp, b'T');
    append_hex8(&mut resp, &mut idx, M0_ROM_STATUS_OK);
    append_hex32(&mut resp, &mut idx, stats.fast_frames);
    append_hex32(&mut resp, &mut idx, stats.commits);
    append_hex32(&mut resp, &mut idx, stats.rx_overruns);
    send_response(port, &mut resp, idx);
}

fn append_probe(resp: &mut [u8], idx: &mut usize, probe: &M0ProbeResult) {
    append_hex8(resp, idx, probe.status);
    append_hex8(resp, idx, probe.read_status);
    append_hex8(resp, idx, probe.ack);
    append_hex32(resp, idx, probe.baud);
    append_hex16(resp, idx, probe.pid);
    append_hex8(resp, idx, probe.blid);
    append_hex8(resp, idx, probe.vector_len);
    for &byte in probe.vector[..probe.vector_len as usize].iter() {
        append_hex8(resp, idx, byte);
    }
}

fn send_simple_response(port: MidiPort, cmd: u8, status: u8) {
    let mut resp = [0u8; 16];
    let mut idx = response_prefix(&mut resp, cmd);
    append_hex8(&mut resp, &mut idx, status);
    send_response(port, &mut resp, idx);
}

fn response_prefix(resp: &mut [u8], cmd: u8) -> usize {
    resp[..NOVATION_HEADER.len()].copy_from_slice(&NOVATION_HEADER);
    resp[6] = M0_RESP_CMD;
    resp[7] = cmd;
    8
}

fn send_response(port: MidiPort, resp: &mut [u8], idx: usize) {
    resp[idx] = 0xf7;
    driver::send_midi(port, &resp[..idx + 1]);
}

fn append_hex8(resp: &mut [u8], idx: &mut usize, value: u8) {
    append_nibble(resp, idx, value >> 4);
    append_nibble(resp, idx, value);
}

fn append_hex16(resp: &mut [u8], idx: &mut usize, value: u16) {
    append_hex8(resp, idx, (value >> 8) as u8);
    append_hex8(resp, idx, value as u8);
}

fn append_hex32(resp: &mut [u8], idx: &mut usize, value: u32) {
    append_hex16(resp, idx, (value >> 16) as u16);
    append_hex16(resp, idx, value as u16);
}

fn append_nibble(resp: &mut [u8], idx: &mut usize, value: u8) {
    let value = value & 0x0f;
    resp[*idx] = if value < 10 {
        b'0' + value
    } else {
        b'A' + value - 10
    };
    *idx += 1;
}
