// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};

use super::grid::Grid;

static GRID: AtomicPtr<Grid> = AtomicPtr::new(ptr::null_mut());

pub fn spawn(spawner: &Spawner, grid: *mut Grid) {
    GRID.store(grid, Ordering::Release);
    spawner.spawn(surface_logic_task().expect("surface_logic_task token"));
}

#[embassy_executor::task]
async fn surface_logic_task() {
    let mut ticker_1khz = Ticker::every(Duration::from_millis(1));
    let mut tick_200hz_divider = 0u8;

    loop {
        ticker_1khz.next().await;

        let grid = GRID.load(Ordering::Acquire);
        if grid.is_null() {
            continue;
        }

        let grid = unsafe { &mut *grid };
        grid.tick_1khz().await;

        tick_200hz_divider = (tick_200hz_divider + 1) % 5;
        if tick_200hz_divider == 0 {
            grid.tick_200hz();
        }
    }
}
