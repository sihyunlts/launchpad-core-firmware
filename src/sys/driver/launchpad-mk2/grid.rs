// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::ptr;

use super::inputs::{GridEvent, Inputs, RAW_BUTTON_BYTES, SHIFT_BYTES_PER_SCAN};
use super::leds::Leds;

const GROUP_COUNT: usize = 4;
const BRIGHT_BIT_ORDER: [usize; 6] = [0, 5, 1, 4, 2, 3];
const GROUP_ORDER: [[u8; GROUP_COUNT]; GROUP_COUNT] =
    [[0, 1, 2, 3], [1, 2, 3, 0], [2, 3, 0, 1], [3, 0, 1, 2]];

const RCC_APB2ENR: *mut u32 = 0x4002_1018 as *mut u32;
const RCC_APB1ENR: *mut u32 = 0x4002_101c as *mut u32;
const GPIOA_CRL: *mut u32 = 0x4001_0800 as *mut u32;
const GPIOB_CRL: *mut u32 = 0x4001_0c00 as *mut u32;
const GPIOB_CRH: *mut u32 = 0x4001_0c04 as *mut u32;
const GPIOA_BSRR: *mut u32 = 0x4001_0810 as *mut u32;
const GPIOB_BSRR: *mut u32 = 0x4001_0c10 as *mut u32;
const GPIOB_ODR: *mut u32 = 0x4001_0c0c as *mut u32;
const SPI2_CR1: *mut u32 = 0x4000_3800 as *mut u32;
const SPI2_CR2: *mut u32 = 0x4000_3804 as *mut u32;
const SPI2_SR: *mut u32 = 0x4000_3808 as *mut u32;
const SPI2_DR: *mut u32 = 0x4000_380c as *mut u32;

const AFIOEN: u32 = 1 << 0;
const IOPAEN: u32 = 1 << 2;
const IOPBEN: u32 = 1 << 3;
const SPI2EN: u32 = 1 << 14;

const SPI_CR1_MSTR: u32 = 1 << 2;
const SPI_CR1_BR_1: u32 = 1 << 4;
const SPI_CR1_SPE: u32 = 1 << 6;
const SPI_CR1_LSBFIRST: u32 = 1 << 7;
const SPI_CR1_SSI: u32 = 1 << 8;
const SPI_CR1_SSM: u32 = 1 << 9;
const SPI_SR_RXNE: u32 = 1 << 0;
const SPI_SR_TXE: u32 = 1 << 1;
const SPI_SR_BSY: u32 = 1 << 7;

pub struct Grid {
    inputs: Inputs,
    leds: Leds,
    selected_group: u8,
    selected_bit: u8,
    group_bank: u8,
    group_phase: u8,
    button_capture_group: u8,
    button_capture_pending: bool,
    shift_tx: [u8; SHIFT_BYTES_PER_SCAN],
    shift_rx: [u8; SHIFT_BYTES_PER_SCAN],
    button_raw: [u8; RAW_BUTTON_BYTES],
}

impl Grid {
    pub fn new() -> Self {
        init_surface_hardware();

        let this = Self {
            inputs: Inputs::new(),
            leds: Leds::new(),
            selected_group: 0,
            selected_bit: 0,
            group_bank: 0,
            group_phase: 0,
            button_capture_group: 0,
            button_capture_pending: false,
            shift_tx: [0xff; SHIFT_BYTES_PER_SCAN],
            shift_rx: [0xff; SHIFT_BYTES_PER_SCAN],
            button_raw: [0; RAW_BUTTON_BYTES],
        };
        this.blank_assert();
        this.deselect_all_groups();
        this
    }

    pub fn poll_event(&mut self) -> Option<GridEvent> {
        self.inputs.poll_event()
    }

    pub fn set_led(&mut self, index: u8, color: u32) {
        self.leds.set_led(index, color);
    }

    pub fn set_led_rgb(&mut self, index: u8, r: u8, g: u8, b: u8) {
        self.leds.set_led_rgb(index, r, g, b);
    }

    pub fn fill(&mut self, color: u32) {
        self.leds.fill(color);
    }

    pub fn brightness(&self) -> u8 {
        self.leds.brightness()
    }

    pub fn set_brightness(&mut self, brightness: u8) {
        self.leds.set_brightness(brightness);
    }

    pub fn blank_phase(&mut self) {
        self.blank_assert();

        self.button_capture_pending = is_button_capture_slot(self.selected_bit);
        if self.button_capture_pending {
            self.button_capture_group = self.selected_group;
        }

        self.advance_scan();
        self.deselect_all_groups();
        self.leds.build_group_payload(
            self.selected_group as usize,
            BRIGHT_BIT_ORDER[self.selected_bit as usize],
            &mut self.shift_tx,
        );
    }

    pub fn null_surface_phase(&mut self) {}

    pub fn ledshift_phase(&mut self) {
        spi2_transfer(&self.shift_tx, &mut self.shift_rx);
    }

    pub fn bright_phase(&mut self) -> u8 {
        let bright_step = self.selected_bit;
        if self.button_capture_pending {
            self.capture_selected_group_buttons();
            self.button_capture_pending = false;
        }
        self.blank_release();
        self.select_group(self.selected_group);
        bright_step
    }

    pub async fn tick_1khz(&mut self) {
        self.inputs.decode_buttons(&self.button_raw);
    }

    pub fn tick_200hz(&mut self) {}

    fn capture_selected_group_buttons(&mut self) {
        let base = self.button_capture_group as usize * SHIFT_BYTES_PER_SCAN;
        self.button_raw[base..base + SHIFT_BYTES_PER_SCAN].copy_from_slice(&self.shift_rx);
    }

    fn advance_scan(&mut self) {
        self.group_phase += 1;
        if self.group_phase as usize == GROUP_COUNT {
            self.group_phase = 0;
            self.selected_bit += 1;
            if self.selected_bit >= 6 {
                self.selected_bit = 0;
                self.group_bank = (self.group_bank + 1) & 3;
            }
        }

        self.selected_group = GROUP_ORDER[self.group_bank as usize][self.group_phase as usize];
    }

    fn blank_assert(&self) {
        unsafe {
            write_reg(GPIOA_BSRR, 1 << 4);
        }
    }

    fn blank_release(&self) {
        unsafe {
            write_reg(GPIOA_BSRR, 1 << (4 + 16));
        }
    }

    fn deselect_all_groups(&self) {
        unsafe {
            write_reg(GPIOB_BSRR, (1 << 0) | (1 << 1) | (1 << 2) | (1 << 10));
        }
    }

    fn select_group(&self, group: u8) {
        unsafe {
            match group {
                0 => write_reg(GPIOB_BSRR, 1 << (0 + 16)),
                1 => write_reg(GPIOB_BSRR, 1 << (1 + 16)),
                2 => write_reg(GPIOB_BSRR, 1 << (2 + 16)),
                _ => write_reg(GPIOB_BSRR, 1 << (10 + 16)),
            }
        }
    }
}

fn is_button_capture_slot(slot: u8) -> bool {
    ((slot & 0xfd) == 1) || slot == 5
}

fn init_surface_hardware() {
    unsafe {
        modify_reg(RCC_APB2ENR, |value| value | AFIOEN | IOPAEN | IOPBEN);
        modify_reg(RCC_APB1ENR, |value| value | SPI2EN);

        let gpioa_crl = set_pin_mode(read_reg(GPIOA_CRL), 4, 0b0001);
        write_reg(GPIOA_CRL, gpioa_crl);

        let mut gpiob_crl = read_reg(GPIOB_CRL);
        gpiob_crl = set_pin_mode(gpiob_crl, 0, 0b0001);
        gpiob_crl = set_pin_mode(gpiob_crl, 1, 0b0001);
        gpiob_crl = set_pin_mode(gpiob_crl, 2, 0b0001);
        write_reg(GPIOB_CRL, gpiob_crl);

        let mut gpiob_crh = read_reg(GPIOB_CRH);
        gpiob_crh = set_pin_mode(gpiob_crh, 10, 0b0001);
        gpiob_crh = set_pin_mode(gpiob_crh, 12, 0b0001);
        gpiob_crh = set_pin_mode(gpiob_crh, 13, 0b1001);
        gpiob_crh = set_pin_mode(gpiob_crh, 14, 0b0100);
        gpiob_crh = set_pin_mode(gpiob_crh, 15, 0b1001);
        write_reg(GPIOB_CRH, gpiob_crh);

        modify_reg(GPIOB_ODR, |value| {
            value | (1 << 0) | (1 << 1) | (1 << 2) | (1 << 10)
        });

        write_reg(SPI2_CR1, 0);
        write_reg(SPI2_CR2, 0);
        write_reg(
            SPI2_CR1,
            SPI_CR1_MSTR
                | SPI_CR1_BR_1
                | SPI_CR1_LSBFIRST
                | SPI_CR1_SSM
                | SPI_CR1_SSI
                | SPI_CR1_SPE,
        );
    }
}

fn spi2_transfer(tx: &[u8; SHIFT_BYTES_PER_SCAN], rx: &mut [u8; SHIFT_BYTES_PER_SCAN]) {
    for (index, &byte) in tx.iter().enumerate() {
        unsafe {
            while read_reg(SPI2_SR) & SPI_SR_TXE == 0 {}
            ptr::write_volatile(SPI2_DR as *mut u8, byte);
            while read_reg(SPI2_SR) & SPI_SR_RXNE == 0 {}
            rx[index] = ptr::read_volatile(SPI2_DR as *const u8);
        }
    }

    unsafe { while read_reg(SPI2_SR) & SPI_SR_BSY != 0 {} }
}

fn set_pin_mode(register: u32, pin: u8, mode: u32) -> u32 {
    let shift = ((pin % 8) as u32) * 4;
    (register & !(0xf << shift)) | (mode << shift)
}

unsafe fn read_reg(reg: *mut u32) -> u32 {
    unsafe { ptr::read_volatile(reg) }
}

unsafe fn write_reg(reg: *mut u32, value: u32) {
    unsafe {
        ptr::write_volatile(reg, value);
    }
}

unsafe fn modify_reg(reg: *mut u32, f: impl FnOnce(u32) -> u32) {
    let value = unsafe { ptr::read_volatile(reg) };
    unsafe {
        ptr::write_volatile(reg, f(value));
    }
}
