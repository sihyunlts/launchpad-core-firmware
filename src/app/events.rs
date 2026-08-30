// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::sys::midi::MidiPort;

#[derive(Copy, Clone)]
pub struct SurfaceEvent {
    pub pressed: bool,
    pub index: u8,
    pub value: u8,
}

#[derive(Copy, Clone)]
pub struct MidiEvent {
    pub port: MidiPort,
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

#[derive(Copy, Clone)]
pub struct AftertouchEvent {
    pub index: u8,
    pub value: u8,
}
