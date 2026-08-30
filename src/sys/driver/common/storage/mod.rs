// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

#[cfg(any(
    feature = "launchpad-x",
    feature = "launchpad-mini-mk3",
    feature = "launchpad-pro-mk3"
))]
pub mod external;

#[cfg(any(
    feature = "launchpad-x",
    feature = "launchpad-mini-mk3",
    feature = "launchpad-pro-mk3"
))]
pub use external::ExtFlash;

#[cfg(any(feature = "launchpad-mk2", feature = "launchpad-pro"))]
pub mod internal;

#[cfg(any(feature = "launchpad-mk2", feature = "launchpad-pro"))]
pub use internal::Flash;
