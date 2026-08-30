// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub const LED_COUNT: usize = 108;

pub mod highspeed;
pub mod legacy;
use super::runtime::M0Link;

pub enum LedSystem {
    Legacy(legacy::Leds),
    Highspeed(highspeed::Leds),
}

impl LedSystem {
    pub fn new_legacy() -> Self {
        Self::Legacy(legacy::Leds::new())
    }

    pub fn new_highspeed() -> Self {
        Self::Highspeed(highspeed::Leds::new())
    }

    pub fn task(&mut self, link: &mut M0Link) {
        match self {
            Self::Legacy(leds) => leds.task(link),
            Self::Highspeed(leds) => leds.task(link),
        }
    }

    pub fn is_highspeed(&self) -> bool {
        matches!(self, Self::Highspeed(_))
    }

    pub fn set_rgb_led(&mut self, index: u8, r: u8, g: u8, b: u8) {
        match self {
            Self::Legacy(leds) => leds.set_rgb_led(index, r, g, b),
            Self::Highspeed(leds) => leds.set_rgb_led(index, r, g, b),
        }
    }

    pub fn set_led(&mut self, index: u8, color: u32) {
        match self {
            Self::Legacy(leds) => leds.set_led(index, color),
            Self::Highspeed(leds) => leds.set_led(index, color),
        }
    }
}
