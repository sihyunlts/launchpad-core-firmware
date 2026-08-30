// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister
// Copyright (C) 2026 ZephyrCodesStuff

use stm32_metapac as pac;

use super::inputs::{GridEvent, Inputs};
use super::leds::Leds;

const SCAN_ROW_LUT: [u8; 96] = [
    0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 3, 0, 1, 2, 3, 0, 1, 2,
    3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1,
    2, 3, 0, 1, 2, 3, 0, 1, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0,
];
const SCAN_PHASE_LUT: [u8; 96] = [
    0, 3, 2, 4, 5, 0, 3, 2, 1, 5, 0, 3, 4, 1, 5, 0, 2, 4, 1, 5, 3, 2, 4, 1, 4, 0, 3, 2, 2, 5, 0, 3,
    3, 1, 5, 0, 0, 4, 1, 5, 5, 2, 4, 1, 1, 3, 2, 4, 2, 4, 0, 3, 3, 2, 5, 0, 0, 3, 1, 5, 5, 0, 4, 1,
    1, 5, 2, 4, 4, 1, 3, 2, 3, 2, 4, 0, 0, 3, 2, 5, 5, 0, 3, 1, 1, 5, 0, 4, 4, 1, 5, 2, 2, 4, 1, 3,
];
const BRIGHTNESS_BASE_LUT: [u64; 9] = [0x77, 0x6d, 0x62, 0x58, 0x4d, 0x43, 0x38, 0x2d, 0x22];
const BRIGHTNESS_PHASE_LUT: [[u64; 6]; 9] = [
    [0x0005, 0x0008, 0x000e, 0x001a, 0x0032, 0x0062],
    [0x0006, 0x000a, 0x0012, 0x0022, 0x0042, 0x0082],
    [0x0007, 0x000c, 0x0016, 0x002a, 0x0052, 0x00a2],
    [0x0008, 0x000e, 0x001a, 0x0032, 0x0062, 0x00c2],
    [0x0009, 0x0010, 0x001e, 0x003a, 0x0072, 0x00e2],
    [0x000a, 0x0012, 0x0022, 0x0042, 0x0082, 0x0102],
    [0x000b, 0x0014, 0x0026, 0x004a, 0x0092, 0x0122],
    [0x000c, 0x0016, 0x002a, 0x0052, 0x00a2, 0x0142],
    [0x000d, 0x0018, 0x002e, 0x005a, 0x00b2, 0x0162],
];
const ROW_MASK: [u16; 4] = [1 << 0, 1 << 1, 1 << 2, 1 << 10];
const MUX_SET_MASK: [u16; 8] = [
    0x0000, 0x0020, 0x0040, 0x0060, 0x0080, 0x00a0, 0x00c0, 0x00e0,
];
const MUX_RESET_MASK: [u16; 8] = [
    0x00e0, 0x00c0, 0x00a0, 0x0080, 0x0060, 0x0040, 0x0020, 0x0000,
];

pub struct Grid {
    scan_slot: u8,
    mux_bank: u8,
    active_mux_bank: u8,
    pressure_pending_bank: u8,
    pressure_pending_valid: bool,
    leds: Leds,
    inputs: Inputs,
}

impl Grid {
    pub fn new() -> Self {
        let mut inputs = Inputs::new();
        init_led_hardware();
        inputs.init_hardware();

        let mut this = Self {
            scan_slot: 0,
            mux_bank: 0,
            active_mux_bank: 0,
            pressure_pending_bank: 0,
            pressure_pending_valid: false,
            leds: Leds::new(),
            inputs,
        };
        this.leds.fill(0x00ff00);
        this
    }

    pub fn prepare_phase(&mut self) {
        if self.pressure_pending_valid
            && self
                .inputs
                .finish_pressure_capture(self.pressure_pending_bank)
        {
            self.pressure_pending_valid = false;
        }

        let row = SCAN_ROW_LUT[self.scan_slot as usize] as usize;
        let phase = SCAN_PHASE_LUT[self.scan_slot as usize] as usize;
        let start = row * 8;
        let prev_slot =
            ((self.scan_slot as usize) + SCAN_PHASE_LUT.len() - 1) % SCAN_PHASE_LUT.len();
        let group = ((prev_slot >> 2) & 0x03) as u8;
        let capture_row = SCAN_ROW_LUT[prev_slot];
        let mux_bank = self.mux_bank;

        self.inputs
            .capture_side(group, capture_row, sample_side_inputs());

        gpio_reset_c(1 << 11);
        gpio_set_b((1 << 0) | (1 << 1) | (1 << 2) | (1 << 10));
        gpio_reset_b(MUX_RESET_MASK[mux_bank as usize]);
        gpio_set_b(MUX_SET_MASK[mux_bank as usize]);

        self.active_mux_bank = mux_bank;
        self.mux_bank = (mux_bank + 1) & 0x07;

        spi3_transfer_8(&self.leds.fb[phase][start..start + 8]);
        self.inputs.start_pressure_capture(self.active_mux_bank);
    }

    pub fn drive_phase(&mut self) {
        let row = SCAN_ROW_LUT[self.scan_slot as usize] as usize;
        let phase = SCAN_PHASE_LUT[self.scan_slot as usize];
        let phase_mask = 1u8 << phase;

        let pc_bsrr = (1 << 11)
            | (if (self.leds.overlay_r & phase_mask) != 0 {
                1 << (7 + 16)
            } else {
                1 << 7
            })
            | (if (self.leds.overlay_g & phase_mask) != 0 {
                1 << (8 + 16)
            } else {
                1 << 8
            })
            | (if (self.leds.overlay_b & phase_mask) != 0 {
                1 << (9 + 16)
            } else {
                1 << 9
            });
        pac::GPIOC
            .bsrr()
            .write_value(pac::gpio::regs::Bsrr(pc_bsrr));
        gpio_reset_b(ROW_MASK[row]);

        self.pressure_pending_bank = self.active_mux_bank;
        self.pressure_pending_valid = true;
    }

    pub fn advance_slot(&mut self) {
        self.scan_slot += 1;
        if self.scan_slot as usize >= SCAN_PHASE_LUT.len() {
            self.scan_slot = 0;
        }
    }

    pub fn frame_complete(&self) -> bool {
        self.scan_slot == 0
    }

    pub fn process_inputs(&mut self) {
        self.inputs.service();
    }

    pub fn poll_event(&mut self) -> Option<GridEvent> {
        self.inputs.pop_event()
    }

    pub fn set_led(&mut self, index: u8, color: u32) {
        self.leds.set_led(flip_index(index), color);
    }

    pub fn set_led_rgb(&mut self, index: u8, r: u8, g: u8, b: u8) {
        self.leds.set_led_rgb(flip_index(index), r, g, b);
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

    pub fn prepare_delay_us(&self) -> u64 {
        BRIGHTNESS_BASE_LUT[self.brightness_index()]
    }

    pub fn drive_delay_us(&self) -> u64 {
        let phase = SCAN_PHASE_LUT[self.scan_slot as usize] as usize;
        BRIGHTNESS_PHASE_LUT[self.brightness_index()][phase]
    }

    fn brightness_index(&self) -> usize {
        self.leds.brightness().min(8) as usize
    }
}

fn flip_index(index: u8) -> u8 {
    (index % 10) + (9 - (index / 10)) * 10
}

const SIDE_INPUT_LUT: [u16; 16] = [
    0x0000, 0x0001, 0x0010, 0x0011, 0x0100, 0x0101, 0x0110, 0x0111, 0x1000, 0x1001, 0x1010, 0x1011,
    0x1100, 0x1101, 0x1110, 0x1111,
];

fn sample_side_inputs() -> u16 {
    let idr = (pac::GPIOC.idr().read().0 & 0x0F) as usize;
    SIDE_INPUT_LUT[idr]
}

fn init_led_hardware() {
    critical_section::with(|_cs| {
        pac::RCC.ahb1enr().modify(|w| {
            w.set_gpioben(true);
            w.set_gpiocen(true);
        });
        pac::RCC.apb1enr().modify(|w| w.set_spi3en(true));
    });

    configure_gpio_af_c(10, 6);
    configure_gpio_af_c(12, 6);

    for pin in [0, 1, 2, 5, 6, 7, 10] {
        configure_gpio_output_b(pin);
    }
    for pin in [7, 8, 9, 11] {
        configure_gpio_output_c(pin);
    }
    for pin in [0, 1, 2, 3] {
        configure_gpio_input_c(pin);
    }

    gpio_set_b((1 << 0) | (1 << 1) | (1 << 2) | (1 << 10));
    gpio_set_c((1 << 7) | (1 << 8) | (1 << 9));
    gpio_reset_c(1 << 11);
    gpio_reset_b((1 << 5) | (1 << 6) | (1 << 7));

    pac::SPI3.cr1().write(|w| {
        w.set_mstr(pac::spi::vals::Mstr::MASTER);
        w.set_ssm(true);
        w.set_ssi(true);
        w.set_br(pac::spi::vals::Br::DIV8);
    });
    pac::SPI3.cr1().modify(|w| w.set_spe(true));
}

fn configure_gpio_af_c(pin: usize, af: u8) {
    pac::GPIOC
        .moder()
        .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::ALTERNATE));
    pac::GPIOC
        .ospeedr()
        .modify(|w| w.set_ospeedr(pin, pac::gpio::vals::Ospeedr::VERY_HIGH_SPEED));
    pac::GPIOC
        .pupdr()
        .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::FLOATING));
    pac::GPIOC.afr(pin / 8).modify(|w| w.set_afr(pin % 8, af));
}

fn configure_gpio_output_b(pin: usize) {
    pac::GPIOB
        .moder()
        .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::OUTPUT));
    pac::GPIOB
        .ospeedr()
        .modify(|w| w.set_ospeedr(pin, pac::gpio::vals::Ospeedr::VERY_HIGH_SPEED));
    pac::GPIOB
        .pupdr()
        .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::FLOATING));
}

fn configure_gpio_output_c(pin: usize) {
    pac::GPIOC
        .moder()
        .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::OUTPUT));
    pac::GPIOC
        .ospeedr()
        .modify(|w| w.set_ospeedr(pin, pac::gpio::vals::Ospeedr::VERY_HIGH_SPEED));
    pac::GPIOC
        .pupdr()
        .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::FLOATING));
}

fn configure_gpio_input_c(pin: usize) {
    pac::GPIOC
        .moder()
        .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::INPUT));
    pac::GPIOC
        .pupdr()
        .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::FLOATING));
}

fn spi3_transfer_8(tx: &[u8]) {
    if tx.len() < 8 {
        return;
    }

    let dr_ptr = pac::SPI3.dr().as_ptr() as *mut u8;
    for byte in &tx[..8] {
        while !pac::SPI3.sr().read().txe() {}
        unsafe { core::ptr::write_volatile(dr_ptr, *byte) };
    }

    while pac::SPI3.sr().read().bsy() {}
    let _ = unsafe { core::ptr::read_volatile(dr_ptr) };
    let _ = pac::SPI3.sr().read();
}

fn gpio_set_b(pins: u16) {
    pac::GPIOB
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr(pins as u32));
}

fn gpio_reset_b(pins: u16) {
    pac::GPIOB
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr((pins as u32) << 16));
}

fn gpio_set_c(pins: u16) {
    pac::GPIOC
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr(pins as u32));
}

fn gpio_reset_c(pins: u16) {
    pac::GPIOC
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr((pins as u32) << 16));
}
