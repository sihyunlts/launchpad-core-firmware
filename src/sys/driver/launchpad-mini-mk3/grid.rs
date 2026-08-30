// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use super::buttons::Buttons;
use super::leds::Leds;
use embassy_stm32::Peri;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::peripherals;
use stm32_metapac as pac;

const SCAN_PHASE_LUT: [u8; 96] = [
    0, 3, 2, 4, 5, 0, 3, 2, 1, 5, 0, 3, 4, 1, 5, 0, 2, 4, 1, 5, 3, 2, 4, 1, 4, 0, 3, 2, 2, 5, 0, 3,
    3, 1, 5, 0, 0, 4, 1, 5, 5, 2, 4, 1, 1, 3, 2, 4, 2, 4, 0, 3, 3, 2, 5, 0, 0, 3, 1, 5, 5, 0, 4, 1,
    1, 5, 2, 4, 4, 1, 3, 2, 3, 2, 4, 0, 0, 3, 2, 5, 5, 0, 3, 1, 1, 5, 0, 4, 4, 1, 5, 2, 2, 4, 1, 3,
];
const SCAN_ROW_LUT: [u8; 96] = [
    0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 3, 0, 1, 2, 3, 0, 1, 2,
    3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1,
    2, 3, 0, 1, 2, 3, 0, 1, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0,
];
const BRIGHTNESS_BASE_LUT: [u64; 11] = [
    0x8c, 0x81, 0x77, 0x6d, 0x62, 0x58, 0x4d, 0x43, 0x38, 0x2d, 0x22,
];

const BRIGHTNESS_PHASE_LUT: [[u64; 6]; 11] = [
    [0x0003, 0x0004, 0x0006, 0x000a, 0x0012, 0x0022],
    [0x0004, 0x0006, 0x000a, 0x0012, 0x0022, 0x0042],
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

pub struct Grid<'d> {
    pa1: Output<'d>,
    pa4: Output<'d>,
    pa8: Output<'d>,
    pb0: Output<'d>,
    pb1: Output<'d>,
    pb2: Output<'d>,
    pb8: Output<'d>,
    pb10: Output<'d>,
    pb12: Output<'d>,
    scan_slot: u8,
    leds: Leds,
    buttons: Buttons,
}

impl<'d> Grid<'d> {
    pub fn new(
        spi2: Peri<'d, peripherals::SPI2>,
        pb13: Peri<'d, peripherals::PB13>,
        pb15: Peri<'d, peripherals::PB15>,
        pb14: Peri<'d, peripherals::PB14>,
        pa1: Peri<'d, peripherals::PA1>,
        pa4: Peri<'d, peripherals::PA4>,
        pa8: Peri<'d, peripherals::PA8>,
        pb0: Peri<'d, peripherals::PB0>,
        pb1: Peri<'d, peripherals::PB1>,
        pb2: Peri<'d, peripherals::PB2>,
        pb8: Peri<'d, peripherals::PB8>,
        pb10: Peri<'d, peripherals::PB10>,
        pb12: Peri<'d, peripherals::PB12>,
    ) -> Self {
        init_spi2_matrix(spi2, pb13, pb15, pb14);

        let leds = Leds::new();
        let buttons = Buttons::new();

        Self {
            pa1: Output::new(pa1, Level::Low, Speed::VeryHigh),
            pa4: Output::new(pa4, Level::Low, Speed::VeryHigh),
            pa8: Output::new(pa8, Level::High, Speed::VeryHigh),
            pb0: Output::new(pb0, Level::High, Speed::VeryHigh),
            pb1: Output::new(pb1, Level::High, Speed::VeryHigh),
            pb2: Output::new(pb2, Level::High, Speed::VeryHigh),
            pb8: Output::new(pb8, Level::High, Speed::VeryHigh),
            pb10: Output::new(pb10, Level::High, Speed::VeryHigh),
            pb12: Output::new(pb12, Level::High, Speed::VeryHigh),
            scan_slot: 0,
            leds,
            buttons,
        }
    }

    fn write_active_low(pin: &mut Output<'d>, low: bool) {
        if low { pin.set_low() } else { pin.set_high() }
    }

    pub fn advance_slot(&mut self) {
        self.scan_slot += 1;
        if self.scan_slot as usize >= SCAN_PHASE_LUT.len() {
            self.scan_slot = 0;
        }
    }

    pub fn prepare_phase(&mut self) {
        let row = SCAN_ROW_LUT[self.scan_slot as usize] as usize;
        let phase = SCAN_PHASE_LUT[self.scan_slot as usize] as usize;
        let start = row * 8;

        let group = ((self.scan_slot >> 2) & 0x03) as u8;
        let capture_row = ((row as u8) + 3) & 0x03;

        let mut txrx = [0u8; 8];
        txrx.copy_from_slice(&self.leds.fb[phase][start..start + 8]);

        self.pa1.set_low();
        self.pa4.set_high();

        self.pb0.set_high();
        self.pb1.set_high();
        self.pb2.set_high();
        self.pb10.set_high();

        spi2_transfer_8(&mut txrx);

        self.buttons.capture_scan(group, capture_row, &txrx);
    }

    pub fn drive_phase(&mut self) {
        let row = SCAN_ROW_LUT[self.scan_slot as usize] as usize;
        let phase = SCAN_PHASE_LUT[self.scan_slot as usize];
        let phase_mask = 1u8 << phase;

        Self::write_active_low(&mut self.pa8, (self.leds.overlay_b & phase_mask) != 0);
        Self::write_active_low(&mut self.pb8, (self.leds.overlay_g & phase_mask) != 0);
        Self::write_active_low(&mut self.pb12, (self.leds.overlay_r & phase_mask) != 0);

        self.pa1.set_high();
        self.pa4.set_low();

        self.pb0.set_high();
        self.pb1.set_high();
        self.pb2.set_high();
        self.pb10.set_high();

        match ROW_MASK[row] {
            x if x == (1 << 0) => self.pb0.set_low(),
            x if x == (1 << 1) => self.pb1.set_low(),
            x if x == (1 << 2) => self.pb2.set_low(),
            x if x == (1 << 10) => self.pb10.set_low(),
            _ => {}
        }
    }

    pub fn frame_complete(&self) -> bool {
        self.scan_slot == 0
    }

    pub fn button_is_valid(&self, index: u8) -> bool {
        self.buttons.is_valid(index)
    }

    pub fn button_is_pressed(&self, index: u8) -> bool {
        self.buttons.is_pressed(index)
    }

    pub fn set_led(&mut self, index: u8, color: u32) {
        let new_index: u8 = (index % 10) + (9 - (index / 10)) * 10;

        self.leds.set_led(new_index, color);
    }

    pub fn set_led_rgb(&mut self, index: u8, r: u8, g: u8, b: u8) {
        let new_index: u8 = (index % 10) + (9 - (index / 10)) * 10;

        self.leds.set_led_rgb(new_index, r, g, b);
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
        let level = self.leds.brightness().min(7);
        let raw = ((level << 5) + 14) & 0xfe;
        ((11 * raw as u16) >> 8) as usize
    }
}

// The Mini Mk3 must receive every byte: MISO carries the scanned button matrix.
// This is deliberately a full-duplex transfer, unlike the Launchpad X LED-only SPI path.
fn init_spi2_matrix(
    spi2: Peri<'_, peripherals::SPI2>,
    pb13: Peri<'_, peripherals::PB13>,
    pb15: Peri<'_, peripherals::PB15>,
    pb14: Peri<'_, peripherals::PB14>,
) {
    // Consume the Embassy ownership tokens before directly configuring the same hardware.
    // No other driver owns SPI2 or these pins after Grid::new returns.
    let _ = (spi2, pb13, pb15, pb14);

    critical_section::with(|_cs| {
        pac::RCC.ahb1enr().modify(|w| w.set_gpioben(true));
        pac::RCC.apb1enr().modify(|w| w.set_spi2en(true));
        pac::RCC.apb1rstr().modify(|w| w.set_spi2rst(true));
        pac::RCC.apb1rstr().modify(|w| w.set_spi2rst(false));
    });

    for pin in [13, 14, 15] {
        pac::GPIOB
            .moder()
            .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::ALTERNATE));
        pac::GPIOB
            .ospeedr()
            .modify(|w| w.set_ospeedr(pin, pac::gpio::vals::Ospeedr::VERY_HIGH_SPEED));
        pac::GPIOB
            .pupdr()
            .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::FLOATING));
        pac::GPIOB.afr(pin / 8).modify(|w| w.set_afr(pin % 8, 5));
    }

    // Mirrors Embassy's blocking SPI configuration: mode 0, MSB first, 8-bit full duplex.
    // APB1 runs at 42 MHz, so DIV4 is the closest hardware rate to the previous 10 MHz request.
    pac::SPI2.cr1().write(|w| {
        w.set_mstr(pac::spi::vals::Mstr::MASTER);
        w.set_br(pac::spi::vals::Br::DIV4);
        w.set_cpha(pac::spi::vals::Cpha::FIRST_EDGE);
        w.set_cpol(pac::spi::vals::Cpol::IDLE_LOW);
        w.set_lsbfirst(pac::spi::vals::Lsbfirst::MSBFIRST);
        w.set_ssm(true);
        w.set_ssi(true);
        w.set_rxonly(pac::spi::vals::Rxonly::FULL_DUPLEX);
        w.set_dff(pac::spi::vals::Dff::BITS8);
        w.set_crcen(false);
        w.set_bidimode(pac::spi::vals::Bidimode::UNIDIRECTIONAL);
        w.set_spe(true);
    });
    pac::SPI2.cr2().write(|w| w.set_ssoe(true));
}

#[inline(always)]
fn spi2_transfer_8(data: &mut [u8; 8]) {
    let dr = pac::SPI2.dr().as_ptr() as *mut u8;

    for byte in data {
        while !pac::SPI2.sr().read().txe() {}
        unsafe { core::ptr::write_volatile(dr, *byte) };
        while !pac::SPI2.sr().read().rxne() {}
        *byte = unsafe { core::ptr::read_volatile(dr) };
    }

    while pac::SPI2.sr().read().bsy() {}
}
