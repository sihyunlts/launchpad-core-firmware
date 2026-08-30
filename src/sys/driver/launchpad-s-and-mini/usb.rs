// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

pub use crate::sys::driver::common::usb::*;

#[cfg(feature = "launchpad-s")]
pub const USB_CONFIG: UsbDeviceConfig = UsbDeviceConfig {
    vendor_id: 0x1235,
    product_id: 0x0020,
    bcd_device: 0x0001,
    ep0_max_packet_size: 64,
    max_power_ma: 60,
    manufacturer: "Focusrite A.E. Ltd",
    product: "Launchpad S",
    serial_number: "COREFW-LPS",
    port1_name: "LPS (DAW)",
    port2_name: "LPS (MIDI)",
    use_ep2_for_out: true,
};

#[cfg(feature = "launchpad-mini-mk1")]
pub const USB_CONFIG: UsbDeviceConfig = UsbDeviceConfig {
    vendor_id: 0x1235,
    product_id: 0x0036,
    bcd_device: 0x0001,
    ep0_max_packet_size: 64,
    max_power_ma: 60,
    manufacturer: "Focusrite A.E. Ltd",
    product: "Launchpad Mini",
    serial_number: "COREFW-MINI",
    port1_name: "MINI (DAW)",
    port2_name: "MINI (MIDI)",
    use_ep2_for_out: true,
};

pub fn init() {
    crate::sys::driver::common::usb::init(&USB_CONFIG);
}
