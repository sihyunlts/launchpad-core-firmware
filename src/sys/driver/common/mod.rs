// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

#[cfg(any(
    feature = "launchpad-x",
    feature = "launchpad-mini-mk3",
    feature = "launchpad-pro-mk3",
    feature = "launchpad-mk2",
    feature = "launchpad-pro"
))]
pub mod storage;
pub mod usb;
