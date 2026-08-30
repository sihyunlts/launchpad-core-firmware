// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::SurfaceEvent;
use crate::app::setup::page::Page;
use crate::app::setup::text::Text;
use crate::driver;
use crate::sys::led;
use crate::sys::rotation::{self, Rotation};
use crate::sys::settings;

const BRIGHTNESS_START: u8 = 31;
const BRIGHTNESS_END: u8 = 38;


#[cfg(not(feature = "launchpad-pro-mk3"))]
const ROTATION_DEFAULT_BUTTON: u8 = 91;
#[cfg(not(feature = "launchpad-pro-mk3"))]
const ROTATION_UPSIDE_DOWN: u8 = 92;
#[cfg(not(feature = "launchpad-pro-mk3"))]
const ROTATION_LEFT_BUTTON: u8 = 93;
#[cfg(not(feature = "launchpad-pro-mk3"))]
const ROTATION_RIGHT_BUTTON: u8 = 94;

#[cfg(feature = "launchpad-pro-mk3")]
const ROTATION_DEFAULT_BUTTON: u8 = 80;
#[cfg(feature = "launchpad-pro-mk3")]
const ROTATION_UPSIDE_DOWN: u8 = 70;
#[cfg(feature = "launchpad-pro-mk3")]
const ROTATION_LEFT_BUTTON: u8 = 91;
#[cfg(feature = "launchpad-pro-mk3")]
const ROTATION_RIGHT_BUTTON: u8 = 92;

pub struct LedsPage {
    text: Text,
}

impl LedsPage {
    pub const fn new() -> Self {
        Self {
            text: Text::new(
                [0b10111110, 0b10110101, 0b10100101, 0b11111110],
                0b11000111,
                0x2020ff,
                0xddddff,
            ),
        }
    }

    fn draw_brightness_selector(&self) {
        let brightness = driver::brightness().min(7);

        for level in 0..8 {
            led::set(BRIGHTNESS_START + level, 0x101010);
        }

        led::set(BRIGHTNESS_START + brightness, 0xccccff);
    }

    #[cfg(feature = "launchpad-pro-mk3")]
    fn draw_mirror_toggle(&self) {
        let mirror_enabled = settings::with(|s| s.mirror_enabled);
        led::set(
            11,
            if mirror_enabled != 0 {
                0x10ff10
            } else {
                0xff1010
            },
        );
    }
    fn draw_rotation_selector(&self) {
        let rotation = rotation::get();

        led::set_raw(
            ROTATION_DEFAULT_BUTTON,
            if rotation == Rotation::Default {
                0xffffff
            } else {
                0x202020
            },
        );
        led::set_raw(
            ROTATION_UPSIDE_DOWN,
            if rotation == Rotation::UpsideDown {
                0xffffff
            } else {
                0x202020
            },
        );
        led::set_raw(
            ROTATION_LEFT_BUTTON,
            if rotation == Rotation::Left {
                0xffffff
            } else {
                0x202020
            },
        );
        led::set_raw(
            ROTATION_RIGHT_BUTTON,
            if rotation == Rotation::Right {
                0xffffff
            } else {
                0x202020
            },
        );
    }
}

impl Page for LedsPage {
    fn on_enter(&mut self) {
        led::set_raw(79, 0x0a0a55);

        self.text.draw();
        self.draw_brightness_selector();
        #[cfg(feature = "launchpad-pro-mk3")]
        self.draw_mirror_toggle();
        self.draw_rotation_selector();
    }

    fn on_surface(&mut self, event: SurfaceEvent) {
        #[cfg(feature = "launchpad-pro-mk3")]
        if event.pressed && event.index == 11 {
            settings::update(|s| {
                s.mirror_enabled = if s.mirror_enabled != 0 { 0 } else { 1 };
            });
            self.draw_mirror_toggle();
            return;
        }

        if !event.pressed || event.index < BRIGHTNESS_START || event.index > BRIGHTNESS_END {
            return;
        }

        let brightness = event.index - BRIGHTNESS_START;

        driver::set_brightness(brightness);
        settings::update(|settings| {
            settings.brightness = brightness;
        });

        self.draw_brightness_selector();
    }

    fn on_surface_raw(&mut self, event: SurfaceEvent) {
        if !event.pressed {
            return;
        }

        let new_rotation = match event.index {
            ROTATION_DEFAULT_BUTTON => Some(Rotation::Default),
            ROTATION_UPSIDE_DOWN => Some(Rotation::UpsideDown),
            ROTATION_LEFT_BUTTON => Some(Rotation::Left),
            ROTATION_RIGHT_BUTTON => Some(Rotation::Right),
            _ => None,
        };

        if let Some(new_rotation) = new_rotation {
            rotation::set(new_rotation);
            for row in 1..=8 {
                for col in 1..=8 {
                    led::set_raw(row * 10 + col, 0);
                }
            }
            self.on_enter();
        }
    }
}
