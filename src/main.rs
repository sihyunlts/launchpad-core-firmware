// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister
// Copyright (C) 2026 ZephyrCodesStuff

#![no_std]
#![no_main]

use panic_halt as _;

pub mod app;
pub mod sys;
pub mod utils;

pub use sys::driver;
