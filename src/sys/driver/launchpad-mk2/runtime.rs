// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use super::grid::Grid;
use super::usb;
use crate::sys::driver::Driver;
use crate::sys::driver::common::storage::Flash;
use crate::sys::midi::MidiPort;

pub struct RuntimeDriver {
    grid: *mut Grid,
    flash: Flash,
}

impl RuntimeDriver {
    pub fn new(grid: &mut Grid, flash: Flash) -> Self {
        Self { grid, flash }
    }

    fn grid(&mut self) -> &mut Grid {
        unsafe { &mut *self.grid }
    }
}

impl Driver for RuntimeDriver {
    fn set_rgb_led(&mut self, index: u8, r: u8, g: u8, b: u8) {
        self.grid().set_led_rgb(index, r, g, b);
    }

    fn set_led(&mut self, index: u8, color: u32) {
        self.grid().set_led(index, color);
    }

    fn fill(&mut self, color: u32) {
        self.grid().fill(color);
    }

    fn brightness(&mut self) -> u8 {
        self.grid().brightness()
    }

    fn set_brightness(&mut self, brightness: u8) {
        self.grid().set_brightness(brightness);
    }

    fn send_midi(&mut self, port: MidiPort, data: &[u8]) {
        match port {
            MidiPort::Din => {}
            MidiPort::Daw | MidiPort::Midi => {
                let _ = usb::enqueue_tx_message(port.as_cable(), data);
            }
        }
    }

    fn flash_size(&mut self) -> u32 {
        self.flash.settings_size()
    }

    fn read_flash(&mut self, offset: u32, data: &mut [u8]) {
        self.flash.read_settings(offset, data);
    }

    fn write_flash(&mut self, offset: u32, data: &[u8]) {
        self.flash.write_settings(offset, data);
    }

    fn device_id(&self) -> u8 {
        105
    }
}
