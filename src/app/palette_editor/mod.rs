// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

mod hue_picker;
mod overview;
mod sl_picker;

use crate::app::AppId;
use crate::app::apptrait::App;
use crate::app::events::{AftertouchEvent, MidiEvent, SurfaceEvent};
use crate::sys::{led, settings};
use crate::utils::palette::PALETTE_NOVATION;

#[cfg(feature = "launchpad-pro-mk3")]
pub(super) const BACK_BUTTON: u8 = 91;
#[cfg(not(feature = "launchpad-pro-mk3"))]
pub(super) const BACK_BUTTON: u8 = 93;
#[cfg(feature = "launchpad-pro-mk3")]
pub(super) const NEXT_BUTTON: u8 = 92;
#[cfg(not(feature = "launchpad-pro-mk3"))]
pub(super) const NEXT_BUTTON: u8 = 94;
#[cfg(feature = "no-setup-btn")]
pub(super) const SAVE_BUTTON: u8 = 95;
pub(super) const FADE_MAX: u8 = 64;

const FADE_STEP: u8 = 4;
const FADE_TICKS: u8 = 10;

#[derive(Copy, Clone, Eq, PartialEq)]
enum Screen {
    Overview,
    HuePicker,
    SlPicker,
}

pub struct PaletteEditorApp {
    screen: Screen,
    half_page: u8,
    selected_index: u8,
    hue: u8,
    anim_progress: u8,
    anim_tick: u8,
    anim_fading_out: bool,
    anim_next_screen: Screen,
    requested_app_switch: Option<AppId>,
}

impl PaletteEditorApp {
    pub const fn new() -> Self {
        Self {
            screen: Screen::Overview,
            half_page: 0,
            selected_index: 0,
            hue: 0,
            anim_progress: 0,
            anim_tick: 0,
            anim_fading_out: false,
            anim_next_screen: Screen::Overview,
            requested_app_switch: None,
        }
    }

    fn dispatch_render(&self) {
        match self.screen {
            Screen::Overview => overview::render(self.half_page, self.anim_progress),
            Screen::HuePicker => hue_picker::render(self.anim_progress),
            Screen::SlPicker => sl_picker::render(self.hue, self.anim_progress),
        }
    }

    fn start_transition(&mut self, new_screen: Screen) {
        self.anim_next_screen = new_screen;
        self.anim_fading_out = true;
        self.anim_tick = 0;
    }

    fn handle_overview_press(&mut self, index: u8) {
        match overview::handle_press(index, self.half_page) {
            overview::Action::None => {}
            #[cfg(feature = "no-setup-btn")]
            overview::Action::SaveAndExit => {
                settings::save();
                self.requested_app_switch = Some(AppId::Setup);
            }
            overview::Action::PreviousPage => {
                self.half_page -= 1;
                self.start_transition(Screen::Overview);
            }
            overview::Action::NextPage => {
                self.half_page += 1;
                self.start_transition(Screen::Overview);
            }
            overview::Action::Select(index) => {
                self.selected_index = index;
                self.start_transition(Screen::HuePicker);
            }
        }
    }

    fn handle_hue_picker_press(&mut self, index: u8) {
        match hue_picker::handle_press(index) {
            hue_picker::Action::None => {}
            hue_picker::Action::Back => self.start_transition(Screen::Overview),
            hue_picker::Action::Select(hue) => {
                self.hue = hue;
                self.start_transition(Screen::SlPicker);
            }
        }
    }

    fn handle_sl_picker_press(&mut self, index: u8) {
        match sl_picker::handle_press(index) {
            sl_picker::Action::None => {}
            sl_picker::Action::Back => self.start_transition(Screen::HuePicker),
            sl_picker::Action::Select {
                saturation,
                lightness,
            } => {
                sl_picker::store(self.selected_index, self.hue, saturation, lightness);
                self.start_transition(Screen::Overview);
            }
        }
    }
}

pub(crate) fn ensure_custom_palette_has_template(palette: u8) {
    if !(4..=6).contains(&palette) {
        return;
    }

    let slot = (palette - 4) as usize;

    settings::update(|settings| {
        let is_zeroed = settings.custom_palette[slot]
            .iter()
            .all(|channel| channel.iter().all(|value| *value == 0));
        let is_erased = settings.custom_palette[slot]
            .iter()
            .all(|channel| channel.iter().all(|value| *value == 0xff));

        if !is_zeroed && !is_erased {
            return;
        }

        for index in 0..128 {
            let (r, g, b) = PALETTE_NOVATION.rgb(index as u8);
            settings.custom_palette[slot][0][index] = r;
            settings.custom_palette[slot][1][index] = g;
            settings.custom_palette[slot][2][index] = b;
        }
    });
}

impl App for PaletteEditorApp {
    fn on_enter(&mut self) {
        ensure_custom_palette_has_template(settings::get().palette);

        self.screen = Screen::Overview;
        self.half_page = 0;
        self.anim_progress = 0;
        self.anim_tick = 0;
        self.anim_fading_out = false;
        self.anim_next_screen = Screen::Overview;
        self.requested_app_switch = None;
    }

    fn on_exit(&mut self) {}

    fn on_surface(&mut self, event: SurfaceEvent) {
        if !event.pressed || self.anim_fading_out || self.anim_progress < FADE_MAX {
            return;
        }

        match self.screen {
            Screen::Overview => self.handle_overview_press(event.index),
            Screen::HuePicker => self.handle_hue_picker_press(event.index),
            Screen::SlPicker => self.handle_sl_picker_press(event.index),
        }
    }

    fn on_midi(&mut self, _event: MidiEvent) {}

    fn on_aftertouch(&mut self, _event: AftertouchEvent) {}

    fn on_tick(&mut self) {
        self.anim_tick = self.anim_tick.saturating_add(1);
        if self.anim_tick < FADE_TICKS {
            return;
        }
        self.anim_tick = 0;

        if self.anim_fading_out {
            if self.anim_progress > FADE_STEP {
                self.anim_progress -= FADE_STEP;
            } else {
                self.anim_progress = 0;
                self.anim_fading_out = false;
                self.screen = self.anim_next_screen;
                led::clear();
            }
            self.dispatch_render();
        } else if self.anim_progress < FADE_MAX {
            self.anim_progress = self.anim_progress.saturating_add(FADE_STEP).min(FADE_MAX);
            self.dispatch_render();
        }
    }

    fn take_requested_app_switch(&mut self) -> Option<AppId> {
        self.requested_app_switch.take()
    }
}

pub(super) fn hsv_to_rgb(h: u8, s: u8, v: u8) -> u32 {
    if v == 0 {
        return 0;
    }
    if s == 0 {
        return ((v as u32) << 16) | ((v as u32) << 8) | v as u32;
    }

    let region = h / 43;
    let remainder = (h - region * 43) * 6;
    let p = ((v as u16 * (255 - s) as u16) >> 8) as u8;
    let q = ((v as u16 * (255 - ((s as u16 * remainder as u16) >> 8))) >> 8) as u8;
    let t = ((v as u16 * (255 - ((s as u16 * (255 - remainder) as u16) >> 8))) >> 8) as u8;

    match region {
        0 => rgb(v, t, p),
        1 => rgb(q, v, p),
        2 => rgb(p, v, t),
        3 => rgb(p, q, v),
        4 => rgb(t, p, v),
        _ => rgb(v, p, q),
    }
}

pub(super) fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

pub(super) fn scale_color(color: u32, factor: u8) -> u32 {
    let factor = factor as u32;
    let r = (((color >> 16) & 0xff) * factor) >> 6;
    let g = (((color >> 8) & 0xff) * factor) >> 6;
    let b = ((color & 0xff) * factor) >> 6;
    (r << 16) | (g << 8) | b
}
