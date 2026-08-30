// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::SurfaceEvent;
use crate::app::setup::page::Page;
use crate::app::setup::text::Text;
use crate::sys::{led, settings};

const AT_MODE_OFF: u8 = 0;
const AT_MODE_POLY: u8 = 1;
const AT_MODE_CHANNEL: u8 = 2;

const MODE_BUTTON_OFF: u8 = 31;
const MODE_BUTTON_POLY: u8 = 32;
const MODE_BUTTON_CHANNEL: u8 = 33;

const CURVE_START: u8 = 21;
const CURVE_END: u8 = 23;

pub struct AftertouchPage {
    text: Text,
}

impl AftertouchPage {
    pub const fn new() -> Self {
        Self {
            text: Text::new(
                [0b01011111, 0b10110010, 0b11111010, 0b10110010],
                0b11100111,
                0x2000ff,
                0xedddff,
            ),
        }
    }

    fn draw(&self) {
        let (at_mode, at_curve) = settings::with(|s| (s.aftertouch_mode, s.aftertouch_curve));
        
        led::set(
            MODE_BUTTON_OFF,
            if at_mode == AT_MODE_OFF {
                0xff1010
            } else {
                0x400101
            },
        );
        
        led::set(
            MODE_BUTTON_POLY,
            if at_mode == AT_MODE_POLY {
                0x10ff10
            } else {
                0x014001
            },
        );
        
        led::set(
            MODE_BUTTON_CHANNEL,
            if at_mode == AT_MODE_CHANNEL {
                0x10ff10
            } else {
                0x014001
            },
        );
        
        if at_mode != AT_MODE_OFF {
            for i in 0..3u8 {
                led::set(CURVE_START + i, 0x101010);
            }
            led::set(CURVE_START + at_curve.min(2), 0x4000ff);
        } else {
            for i in 0..3u8 {
                led::set(CURVE_START + i, 0x000000);
            }
        }
    }
}

impl Page for AftertouchPage {
    fn on_enter(&mut self) {
        led::set_raw(59, 0x150055);
        self.text.draw();
        self.draw();
    }

    fn on_surface(&mut self, event: SurfaceEvent) {
        if !event.pressed {
            return;
        }

        match event.index {
            MODE_BUTTON_OFF => {
                settings::update(|s| s.aftertouch_mode = AT_MODE_OFF);
                self.draw();
            }
            MODE_BUTTON_POLY => {
                settings::update(|s| s.aftertouch_mode = AT_MODE_POLY);
                self.draw();
            }
            MODE_BUTTON_CHANNEL => {
                settings::update(|s| s.aftertouch_mode = AT_MODE_CHANNEL);
                self.draw();
            }
            CURVE_START..=CURVE_END => {
                // Changing the curve only makes sense when aftertouch is active
                if settings::with(|s| s.aftertouch_mode) == AT_MODE_OFF {
                    return;
                }
                let new_curve = event.index - CURVE_START;
                settings::update(|s| s.aftertouch_curve = new_curve);
                self.draw();
            }
            _ => {}
        }
    }
}
