// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

#![no_std]
#![no_main]

mod app;
mod boot;
mod hw;
mod runtime;
mod surface;
mod usb;

use cortex_m_rt::entry;
use firmware_core::app::{AppHost, AppId, SurfaceEvent};
use firmware_core::driver;
use panic_halt as _;
use runtime::RuntimeDriver;
use surface::Surface;

type LaunchpadMiniMk1AppHost = AppHost<boot::BootApp, app::LiveApp>;

#[entry]
fn main() -> ! {
    unsafe {
        core::ptr::write_volatile(0xE000_E010 as *mut u32, 0);
        core::ptr::write_volatile(0xE000_ED04 as *mut u32, 1 << 25);
        let nvic = &*cortex_m::peripheral::NVIC::PTR;
        for i in 0..2 {
            nvic.icer[i].write(0xffff_ffff);
            nvic.icpr[i].write(0xffff_ffff);
        }

        (*cortex_m::peripheral::SCB::PTR)
            .vtor
            .write(hw::APP_VECTOR_TABLE);
    }

    hw::init_clocks();
    init_board_gpios();
    usb::init();
    usb_enable_pullup();

    static mut SURFACE: Surface = Surface::new();
    static mut RUNTIME: RuntimeDriver = RuntimeDriver::new(core::ptr::null_mut());
    static mut APP_HOST: LaunchpadMiniMk1AppHost =
        AppHost::new(AppId::Boot, boot::new(), app::LiveApp::new());

    let surface = unsafe { &mut *core::ptr::addr_of_mut!(SURFACE) };
    surface.init();

    unsafe {
        let runtime = &mut *core::ptr::addr_of_mut!(RUNTIME);
        runtime.set_surface(surface as *mut Surface);
        driver::install(runtime);
        (&mut *core::ptr::addr_of_mut!(APP_HOST)).init();
    }

    surface.start_scan();
    hw::init_tick_timer();

    unsafe {
        cortex_m::interrupt::enable();
    }

    loop {
        cortex_m::interrupt::free(|_| unsafe {
            while let Some(event) = (&mut *core::ptr::addr_of_mut!(SURFACE)).poll_event() {
                let event = match event {
                    surface::SurfaceEvent::Press { index, value } => SurfaceEvent {
                        pressed: true,
                        index,
                        value,
                    },
                    surface::SurfaceEvent::Release { index } => SurfaceEvent {
                        pressed: false,
                        index,
                        value: 0,
                    },
                };
                (&mut *core::ptr::addr_of_mut!(APP_HOST)).route_surface_event(event);
            }
        });

        usb::poll();

        while let Some(event) = usb::dequeue_midi_event() {
            unsafe {
                (&mut *core::ptr::addr_of_mut!(APP_HOST)).route_midi_event(event);
            }
        }

        if hw::take_events(hw::EVENT_1KHZ) != 0 {
            unsafe {
                (&mut *core::ptr::addr_of_mut!(SURFACE)).tick_1khz();
                (&mut *core::ptr::addr_of_mut!(APP_HOST)).route_tick_event();
            }
        }

        if hw::take_events(hw::EVENT_200HZ | hw::EVENT_20HZ) != 0 {
            // Reserved for USB polling and lower-rate surface work once raw USB MIDI is in place.
        }

        cortex_m::asm::wfi();
    }
}

fn init_board_gpios() {
    unsafe {
        hw::init_gpio_clocks();
        hw::write_reg(hw::GPIOA_ODR, 0);
        hw::write_reg(hw::GPIOA_CRL, 0x1491_1111);
        hw::write_reg(hw::GPIOA_CRH, 0x4444_4111);
        hw::write_reg(hw::GPIOB_ODR, 7);
        hw::write_reg(hw::GPIOB_CRL, 0x1114_4111);
        hw::write_reg(hw::GPIOB_CRH, 0x9191_1111);
        hw::write_reg(hw::GPIOC_CRL, 0x1111_1111);
        hw::write_reg(hw::GPIOC_CRH, 0x1111_1111);
        hw::write_reg(hw::GPIOD_CRL, 0x1111_1111);
        hw::write_reg(hw::GPIOD_CRH, 0x1111_1111);
        usb_disable_pullup();
        cortex_m::asm::delay(hw::SYSCLK_HZ / 50);
    }
}

fn usb_enable_pullup() {
    unsafe {
        hw::write_reg(hw::GPIOA_BSRR, 1 << 10);
    }
}

fn usb_disable_pullup() {
    unsafe {
        hw::write_reg(hw::GPIOA_BSRR, 1 << (10 + 16));
    }
}
