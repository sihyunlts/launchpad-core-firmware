// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    if env::var_os("CARGO_FEATURE_LAUNCHPAD_X").is_some() {
        fs::copy("src/sys/driver/launchpad-x/memory.x", out.join("memory.x")).unwrap();
        fs::copy("src/sys/driver/launchpad-x/version.x", out.join("version.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-x/memory.x");
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-x/version.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tversion.x");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
    } else if env::var_os("CARGO_FEATURE_LAUNCHPAD_MINI_MK3").is_some() {
        fs::copy("src/sys/driver/launchpad-mini-mk3/memory.x", out.join("memory.x")).unwrap();
        fs::copy("src/sys/driver/launchpad-mini-mk3/version.x", out.join("version.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-mini-mk3/memory.x");
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-mini-mk3/version.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tversion.x");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
    } else if env::var_os("CARGO_FEATURE_LAUNCHPAD_MK2").is_some() {
        fs::copy("src/sys/driver/launchpad-mk2/memory.x", out.join("memory.x")).unwrap();
        fs::copy("src/sys/driver/launchpad-mk2/version.x", out.join("version.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-mk2/memory.x");
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-mk2/version.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tversion.x");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
    } else if env::var_os("CARGO_FEATURE_LAUNCHPAD_PRO").is_some() {
        fs::copy("src/sys/driver/launchpad-pro/memory.x", out.join("memory.x")).unwrap();
        fs::copy("src/sys/driver/launchpad-pro/version.x", out.join("version.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-pro/memory.x");
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-pro/version.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tversion.x");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
    } else if env::var_os("CARGO_FEATURE_LAUNCHPAD_PRO_MK3").is_some() {
        fs::copy("src/sys/driver/launchpad-pro-mk3/memory.x", out.join("memory.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-pro-mk3/memory.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
    } else if env::var_os("CARGO_FEATURE_LAUNCHPAD_S").is_some()
        || env::var_os("CARGO_FEATURE_LAUNCHPAD_MINI_MK1").is_some()
    {
        fs::copy("src/sys/driver/launchpad-s-and-mini/memory.x", out.join("memory.x")).unwrap();
        fs::copy("src/sys/driver/launchpad-s-and-mini/device.x", out.join("device.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-s-and-mini/memory.x");
        println!("cargo:rerun-if-changed=src/sys/driver/launchpad-s-and-mini/device.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
    }
}
