// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub mod page;
pub mod page_init;
pub mod page_leds;

#[cfg(feature = "pressure-sensitive")]
pub mod page_aftertouch;
#[cfg(feature = "pressure-sensitive")]
pub mod page_velocity;

mod text;

use crate::app::AppId;
use crate::app::apptrait::App;
use crate::app::events::{AftertouchEvent, MidiEvent, SurfaceEvent};
use crate::app::setup::page::{Page, PageId};
use crate::app::setup::page_init::InitPage;
use crate::app::setup::page_leds::LedsPage;
use crate::driver;
use crate::sys::led;
use crate::sys::settings;

#[cfg(feature = "pressure-sensitive")]
use crate::app::setup::page_aftertouch::AftertouchPage;
#[cfg(feature = "pressure-sensitive")]
use crate::app::setup::page_velocity::VelocityPage;

pub struct SetupApp {
    page: PageId,
    init_page: InitPage,
    leds_page: LedsPage,
    #[cfg(feature = "pressure-sensitive")]
    velocity_page: VelocityPage,
    #[cfg(feature = "pressure-sensitive")]
    aftertouch_page: AftertouchPage,
}

impl SetupApp {
    pub const fn new() -> Self {
        Self {
            page: PageId::Init,
            init_page: InitPage::new(),
            leds_page: LedsPage::new(),
            #[cfg(feature = "pressure-sensitive")]
            velocity_page: VelocityPage::new(),
            #[cfg(feature = "pressure-sensitive")]
            aftertouch_page: AftertouchPage::new(),
        }
    }

    fn current_page_mut(&mut self) -> &mut dyn Page {
        match self.page {
            PageId::Init => &mut self.init_page,
            PageId::Leds => &mut self.leds_page,
            #[cfg(feature = "pressure-sensitive")]
            PageId::Velocity => &mut self.velocity_page,
            #[cfg(feature = "pressure-sensitive")]
            PageId::Aftertouch => &mut self.aftertouch_page,
        }
    }

    fn page_for_button(index: u8) -> Option<PageId> {
        match index {
            89 => Some(PageId::Init),
            79 => Some(PageId::Leds),
            #[cfg(feature = "pressure-sensitive")]
            69 => Some(PageId::Velocity),
            #[cfg(feature = "pressure-sensitive")]
            59 => Some(PageId::Aftertouch),
            _ => None,
        }
    }

    fn render_page_tabs() {
        // Init and Leds are always present
        led::set_raw(89, 0x101014);
        led::set_raw(
            90,
            if driver::highspeed_leds_enabled() {
                0x000000
            } else {
                0xff0000
            },
        );
        led::set_raw(79, 0x101014);

        // Velocity and Aftertouch tabs only on pressure-sensitive devices
        #[cfg(feature = "pressure-sensitive")]
        led::set_raw(69, 0x101014);
        #[cfg(feature = "pressure-sensitive")]
        led::set_raw(59, 0x101014);

        #[cfg(feature = "no-setup-btn")]
        led::pulse_raw(95, 0xff0000);
    }

    /// Redraws the current page.
    fn redraw_current_page(&mut self) {
        led::clear();
        Self::render_page_tabs();
        self.current_page_mut().on_enter();
    }

    pub fn set_current_mode(&mut self, app: AppId) {
        self.init_page.set_current_mode(app);
    }

    pub fn finish_setup(&mut self) -> Option<AppId> {
        led::clear();
        settings::save();
        self.init_page.take_selected_mode()
    }
}

impl App for SetupApp {
    fn on_enter(&mut self) {
        Self::render_page_tabs();
        self.current_page_mut().on_enter();
    }

    fn on_exit(&mut self) {}

    fn on_surface(&mut self, event: SurfaceEvent) {
        // `event.index` here has already been translated for the current
        // rotation (main grid only). Edge buttons are handled in
        // `on_surface_raw` below since they never rotate in setup.
        self.current_page_mut().on_surface(event);
    }

    fn on_surface_raw(&mut self, event: SurfaceEvent) {
        if event.index % 10 == 9 && event.pressed {
            let Some(page) = Self::page_for_button(event.index) else {
                return;
            };

            self.page = page;
            self.redraw_current_page();
            return;
        }

        self.current_page_mut().on_surface_raw(event);
    }

    fn on_midi(&mut self, _event: MidiEvent) {}

    fn on_aftertouch(&mut self, _event: AftertouchEvent) {}

    fn on_tick(&mut self) {
        self.current_page_mut().on_tick();
    }

    fn take_requested_app_switch(&mut self) -> Option<AppId> {
        self.init_page.take_requested_app_switch()
    }
}
