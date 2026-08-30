// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use crate::app::AppId;
use crate::app::events::{AftertouchEvent, MidiEvent, SurfaceEvent};

pub trait App {
    fn on_enter(&mut self);
    fn on_exit(&mut self);

    fn on_surface(&mut self, event: SurfaceEvent);
    fn on_midi(&mut self, event: MidiEvent);
    fn on_aftertouch(&mut self, event: AftertouchEvent);

    // Same surface (button) event as `on_surface` but `event.index` is always the raw physical index, never translated for the current rotation.
    // Most apps don't need this but it exists for the setup menu, whose edge buttons (page tabs, orientation toggle, exit hold-button)
    // must always stay in the same physical place regardless of the current rotation.
    fn on_surface_raw(&mut self, _event: SurfaceEvent) {}

    fn on_tick(&mut self);

    fn take_requested_app_switch(&mut self) -> Option<AppId> {
        None
    }
}
