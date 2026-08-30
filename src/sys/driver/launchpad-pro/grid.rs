// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

//! Surface hardware glue: GPIO/SPI2/ADC/DMA setup, the four LED-scan phase
//! callbacks driven by `leds::TIM3`, ADC bank scanning for the pressure
//! pads, and VBUS-based `PowerMode` detection.
//!
//! This is a clean-room reimplementation of the reference Launchpad Pro
//! firmware's surface driver, reverse engineered from the original firmware
//! disassembly. The key change versus a naive reimplementation is that the
//! LED shift-register transfer (SPI2, full duplex with the switch matrix
//! read-back) is driven by DMA (SPI2 TX = DMA1 Channel 5, SPI2 RX = DMA1
//! Channel 4) instead of a blocking busy-wait inside the TIM3 interrupt.
//! Blocking there stalls the CPU for a variable amount of time depending on
//! bus/interrupt contention, which stretches the very short "bright" pulses
//! used for low bit-planes (e.g. bit 0, ~2-5 ticks) by an amount that swamps
//! their nominal duration - this is what caused the visible low-brightness
//! flicker. Using DMA lets the transfer complete deterministically off the
//! CPU, and `leds.rs` gates unblanking on the transfer's completion
//! (`DMAFinished`) rather than assuming a fixed duration.

use super::inputs::{GridEvent, Inputs};
use super::leds::{self, Leds};
use stm32_metapac as pac;

const GROUP_COUNT: usize = 4;
const SHIFT_BYTES_PER_SCAN: usize = 10;
const BRIGHT_BIT_ORDER: [usize; 6] = [0, 5, 1, 4, 2, 3];

const RAW_INDICES: [usize; GROUP_COUNT] = [0, 9, 17, 25];

const DMA1_CH4_ALL_FLAGS: u32 = 0xf << 12;
const DMA1_CH5_ALL_FLAGS: u32 = 0xf << 16;

const SPI_CR1_MSTR: u32 = 1 << 2;
const SPI_CR1_CPOL: u32 = 1 << 1;
const SPI_CR1_CPHA: u32 = 1 << 0;
const SPI_CR1_BR_0: u32 = 1 << 3;
const SPI_CR1_BR_1: u32 = 1 << 4;
const SPI_CR1_SPE: u32 = 1 << 6;
const SPI_CR1_LSBFIRST: u32 = 1 << 7;
const SPI_CR1_SSI: u32 = 1 << 8;
const SPI_CR1_SSM: u32 = 1 << 9;
const SPI_CR2_RXDMAEN: u32 = 1 << 0;
const SPI_CR2_TXDMAEN: u32 = 1 << 1;

const ADC_SR_EOC: u32 = 1 << 1;
const ADC_CR1_SCAN: u32 = 1 << 8;
const ADC_CR2_ADON: u32 = 1 << 0;
const ADC_CR2_CAL: u32 = 1 << 2;
const ADC_CR2_RSTCAL: u32 = 1 << 3;
const ADC_CR2_EXTTRIG: u32 = 1 << 20;
const ADC_CR2_SWSTART: u32 = 1 << 22;
const ADC_CR2_EXTSEL_SWSTART: u32 = 0b111 << 17;
const ADC_CR2_DMA: u32 = 1 << 8;
const DMA_CCR_TCIE: u32 = 1 << 1;
const DMA_CCR_DIR: u32 = 1 << 4;
const DMA_CCR_MINC: u32 = 1 << 7;
const DMA_CCR_PSIZE_16: u32 = 1 << 8;
const DMA_CCR_MSIZE_16: u32 = 1 << 10;

const DMA_IFCR_CTCIF1: u32 = 1 << 1;
const ADC_SEQUENCE: [u8; 16] = [11, 10, 13, 12, 1, 0, 3, 2, 5, 4, 7, 6, 15, 14, 8, 9];

// VBUS/PowerMode detection: GPIOB pin 9 (mask 0x200). High = self-powered
// (PowerMode 2), low = bus-powered (PowerMode 1). Confirmed only after 3
// consecutive agreeing samples, matching the reference firmware's filter.
const VBUS_PIN_MASK: u32 = 1 << 9;
const VBUS_CONFIRM_SAMPLES: u8 = 3;

pub struct Grid {
    inputs: Inputs,
    leds: Leds,
    selected_group: u8,
    selected_bit: u8,
    shift_tx: [u8; SHIFT_BYTES_PER_SCAN],
    shift_rx: [u8; SHIFT_BYTES_PER_SCAN],
    adc_bank: u8,
    adc_buffer: [u16; 16],
    setup_accum: bool,
    vbus_last_raw: bool,
    vbus_stable_count: u8,
    vbus_confirmed_high: bool,
}

impl Grid {
    pub fn new() -> Self {
        init_surface_hardware();
        init_adc_hardware();

        let initial_vbus_high = read_vbus_raw();

        let this = Self {
            inputs: Inputs::new(),
            leds: Leds::new(),
            selected_group: 0,
            selected_bit: 0,
            shift_tx: [0xff; SHIFT_BYTES_PER_SCAN],
            shift_rx: [0xff; SHIFT_BYTES_PER_SCAN],
            adc_bank: 0,
            adc_buffer: [0; 16],
            setup_accum: false,
            vbus_last_raw: initial_vbus_high,
            vbus_stable_count: VBUS_CONFIRM_SAMPLES,
            vbus_confirmed_high: initial_vbus_high,
        };
        this.blank_assert();
        this.deselect_all_groups();
        this.set_adc_bank_lines(0);

        // One-time unfiltered initial PowerMode assignment, matching the
        // reference's `init_exti` behaviour.
        leds::set_power_mode(if initial_vbus_high { 2 } else { 1 });

        this
    }

    pub fn poll_event(&mut self) -> Option<GridEvent> {
        self.inputs.poll_event()
    }

    // NOTE: LED buffer writes are deliberately NOT wrapped in a critical
    // section. The TIM3 scan ISR only ever *reads* the payload buffer, so
    // the worst case of a concurrent write is a torn read that shows one or
    // two LEDs a slightly wrong colour for a single ~sub-millisecond
    // subframe - visually imperceptible. Disabling interrupts here (as the
    // previous implementation did) instead stalls the TIM3 ISR for the whole
    // duration of the write, which stretches whichever "bright" pulse is
    // currently active and produces exactly the kind of update-correlated
    // flicker we are trying to eliminate. The reference firmware likewise
    // writes its LED buffer from the main context with no critical section.
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

    // --- TIM3 phase callbacks, invoked from `leds::TIM3` -------------------

    /// SurfaceMode 0 (BLANK): assert blank, deselect every group line and
    /// build the shift-out payload for the (group, bright-bit) pair that is
    /// about to be scanned.
    pub fn blank_phase(&mut self) {
        self.blank_assert();
        self.deselect_all_groups();
        self.leds.build_group_payload(
            self.selected_group as usize,
            BRIGHT_BIT_ORDER[self.selected_bit as usize],
            &mut self.shift_tx,
        );
    }

    /// SurfaceMode 1 (NULLSURFACE): (re-)arm the DMA transfer-count
    /// registers for both the TX (LED data out, Channel 5) and RX (switch
    /// data in, Channel 4) sides, without starting the transfer yet.
    pub fn null_surface_phase(&mut self) {
        pac::DMA1.ch(3).cr().modify(|w| w.set_en(false));
        pac::DMA1.ch(4).cr().modify(|w| w.set_en(false));
        pac::DMA1.ifcr().write_value(pac::bdma::regs::Isr(
            DMA1_CH4_ALL_FLAGS | DMA1_CH5_ALL_FLAGS,
        ));

        pac::DMA1
            .ch(3)
            .par()
            .write_value(pac::SPI2.dr().as_ptr() as u32);
        pac::DMA1
            .ch(3)
            .mar()
            .write_value(self.shift_rx.as_ptr() as u32);
        pac::DMA1
            .ch(3)
            .ndtr()
            .write_value(pac::bdma::regs::Ndtr(SHIFT_BYTES_PER_SCAN as u32));
        pac::DMA1
            .ch(3)
            .cr()
            .write_value(pac::bdma::regs::Cr(DMA_CCR_MINC | DMA_CCR_TCIE));

        pac::DMA1
            .ch(4)
            .par()
            .write_value(pac::SPI2.dr().as_ptr() as u32);
        pac::DMA1
            .ch(4)
            .mar()
            .write_value(self.shift_tx.as_ptr() as u32);
        pac::DMA1
            .ch(4)
            .ndtr()
            .write_value(pac::bdma::regs::Ndtr(SHIFT_BYTES_PER_SCAN as u32));
        pac::DMA1
            .ch(4)
            .cr()
            .write_value(pac::bdma::regs::Cr(DMA_CCR_MINC | DMA_CCR_DIR));
    }

    /// SurfaceMode 2 (LEDSHIFT): start the SPI2 full-duplex DMA transfer.
    /// Non-blocking - completion is signalled asynchronously via the
    /// DMA1 Channel 4 (RX) interrupt, which sets `DMAFinished` in `leds.rs`.
    pub fn ledshift_phase(&mut self) {
        // Enable RX before TX so the first incoming byte is never missed.
        pac::DMA1.ch(3).cr().modify(|w| w.set_en(true));
        pac::DMA1.ch(4).cr().modify(|w| w.set_en(true));
    }

    /// SurfaceMode 3 (BRIGHT): only called once the previous shift's DMA
    /// transfer has completed. Releases blank, selects the current group's
    /// line, captures switch data from the just-completed shift-in on the
    /// first bright-bit of the frame, and advances the scan position.
    pub fn bright_phase(&mut self) -> u8 {
        let bright_step = self.selected_bit;
        self.blank_release();
        self.select_group(self.selected_group);
        if self.selected_bit == 0 {
            self.capture_switch_group(self.selected_group as usize);
        }
        self.advance_scan();
        bright_step
    }

    pub fn tick_1khz_collect(&mut self) {
        self.collect_adc_scan();
        self.inputs.tick_1khz();
        self.poll_power_mode();
    }

    pub fn tick_1khz_start(&mut self) {
        self.start_adc_scan();
    }

    pub fn tick_200hz(&mut self) {
        self.inputs.tick_200hz();
    }

    pub fn capture_pad_velocity(&mut self, sensor: usize, value: u8) {
        self.inputs.capture_pad_velocity(sensor, value);
    }

    pub fn capture_pad_aftertouch(&mut self, sensor: usize, value: u8) {
        self.inputs.capture_pad_aftertouch(sensor, value);
    }

    pub fn capture_switch_entry(&mut self, entry: usize, pressed: bool) {
        self.inputs.capture_switch_entry(entry, pressed);
    }

    fn capture_switch_group(&mut self, group: usize) {
        let raw_base = RAW_INDICES[group];
        let first = self.shift_rx[0];
        let second = self.shift_rx[1];

        self.inputs.set_switch_raw(raw_base, (first >> 7) & 1 != 0);
        self.inputs
            .set_switch_raw(raw_base + 1, (first >> 6) & 1 != 0);
        self.inputs
            .set_switch_raw(raw_base + 2, (first >> 5) & 1 != 0);
        self.inputs
            .set_switch_raw(raw_base + 3, (first >> 4) & 1 != 0);

        if (first >> 3) & 1 != 0 {
            self.setup_accum = true;
        }

        let second_base = if group == 0 {
            // raw_base + 4 is managed by the accumulator below
            raw_base + 5
        } else {
            raw_base + 4
        };

        if group == 3 {
            self.inputs.set_switch_raw(4, self.setup_accum);
            self.setup_accum = false;
        }

        self.inputs
            .set_switch_raw(second_base, (second >> 7) & 1 != 0);
        self.inputs
            .set_switch_raw(second_base + 1, (second >> 6) & 1 != 0);
        self.inputs
            .set_switch_raw(second_base + 2, (second >> 5) & 1 != 0);
        self.inputs
            .set_switch_raw(second_base + 3, (second >> 4) & 1 != 0);
    }

    fn advance_scan(&mut self) {
        self.selected_group = (self.selected_group + 1) % (GROUP_COUNT as u8);
        if self.selected_group == 0 {
            self.selected_bit += 1;
            if self.selected_bit >= 6 {
                self.selected_bit = 0;
            }
        }
    }

    pub fn start_adc_scan(&mut self) {
        pac::DMA1.ch(0).cr().modify(|w| w.set_en(false));
        pac::DMA1
            .ch(0)
            .mar()
            .write_value(self.adc_buffer.as_ptr() as u32);
        pac::DMA1
            .ch(0)
            .ndtr()
            .write_value(pac::bdma::regs::Ndtr(16));
        pac::DMA1
            .ifcr()
            .write_value(pac::bdma::regs::Isr(DMA_IFCR_CTCIF1));
        pac::DMA1.ch(0).cr().modify(|w| w.set_en(true));

        start_adc_conversion();
    }

    pub fn collect_adc_scan(&mut self) {
        while pac::DMA1.ch(0).ndtr().read().ndt() != 0 {}

        self.inputs
            .capture_adc_bank(self.adc_bank as usize, &self.adc_buffer);
        self.adc_bank = (self.adc_bank + 1) & 3;
        self.set_adc_bank_lines(self.adc_bank);
        if self.adc_bank == 0 {
            self.inputs.accumulate_adc_max();
        }
    }

    /// Debounced VBUS read: only updates `PowerMode` once 3 consecutive
    /// samples agree on a new level, matching the reference firmware's
    /// filter and avoiding spurious PowerMode flips from a noisy/bouncing
    /// VBUS line while a cable is being inserted or removed.
    fn poll_power_mode(&mut self) {
        let raw = read_vbus_raw();
        if raw == self.vbus_last_raw {
            if self.vbus_stable_count < VBUS_CONFIRM_SAMPLES {
                self.vbus_stable_count += 1;
            }
        } else {
            self.vbus_last_raw = raw;
            self.vbus_stable_count = 1;
        }

        if self.vbus_stable_count >= VBUS_CONFIRM_SAMPLES && self.vbus_confirmed_high != raw {
            self.vbus_confirmed_high = raw;
            leds::set_power_mode(if raw { 2 } else { 1 });
        }
    }

    fn set_adc_bank_lines(&self, bank: u8) {
        match bank & 3 {
            0 => pac::GPIOC
                .brr()
                .write_value(pac::gpio::regs::Brr((1 << 8) | (1 << 9))),
            1 => {
                pac::GPIOC.bsrr().write_value(pac::gpio::regs::Bsrr(1 << 9));
                pac::GPIOC.brr().write_value(pac::gpio::regs::Brr(1 << 8));
            }
            2 => {
                pac::GPIOC.brr().write_value(pac::gpio::regs::Brr(1 << 9));
                pac::GPIOC.bsrr().write_value(pac::gpio::regs::Bsrr(1 << 8));
            }
            _ => pac::GPIOC
                .bsrr()
                .write_value(pac::gpio::regs::Bsrr((1 << 8) | (1 << 9))),
        }
    }

    fn blank_assert(&self) {
        pac::GPIOB
            .bsrr()
            .write_value(pac::gpio::regs::Bsrr(1 << 12));
        pac::GPIOB.brr().write_value(pac::gpio::regs::Brr(1 << 8));
    }

    fn blank_release(&self) {
        pac::GPIOB.brr().write_value(pac::gpio::regs::Brr(1 << 12));
        pac::GPIOB.bsrr().write_value(pac::gpio::regs::Bsrr(1 << 8));
    }

    fn deselect_all_groups(&self) {
        pac::GPIOC
            .bsrr()
            .write_value(pac::gpio::regs::Bsrr((1 << 10) | (1 << 11) | (1 << 12)));
        pac::GPIOD.bsrr().write_value(pac::gpio::regs::Bsrr(1 << 2));
    }

    fn select_group(&self, group: u8) {
        match group {
            0 => pac::GPIOC
                .bsrr()
                .write_value(pac::gpio::regs::Bsrr(1 << (10 + 16))),
            1 => pac::GPIOC
                .bsrr()
                .write_value(pac::gpio::regs::Bsrr(1 << (11 + 16))),
            2 => pac::GPIOC
                .bsrr()
                .write_value(pac::gpio::regs::Bsrr(1 << (12 + 16))),
            _ => pac::GPIOD
                .bsrr()
                .write_value(pac::gpio::regs::Bsrr(1 << (2 + 16))),
        }
    }
}

fn read_vbus_raw() -> bool {
    pac::GPIOB.idr().read().0 & VBUS_PIN_MASK != 0
}

fn init_surface_hardware() {
    pac::RCC.apb2enr().modify(|w| {
        w.set_afioen(true);
        w.set_gpioaen(true);
        w.set_gpioben(true);
        w.set_gpiocen(true);
        w.set_gpioden(true);
    });
    pac::RCC.apb1enr().modify(|w| w.set_spi2en(true));

    pac::GPIOA.cr(0).write_value(pac::gpio::regs::Cr(0));

    pac::GPIOB.cr(0).modify(|w| {
        w.0 = set_pin_mode(w.0, 0, 0b0000);
        w.0 = set_pin_mode(w.0, 1, 0b0000);
        w.0 = set_pin_mode(w.0, 3, 0b0100);
        w.0 = set_pin_mode(w.0, 4, 0b0100);
        w.0 = set_pin_mode(w.0, 5, 0b0001);
        w.0 = set_pin_mode(w.0, 6, 0b0001);
        w.0 = set_pin_mode(w.0, 7, 0b0001);
    });
    pac::GPIOB.cr(1).modify(|w| {
        w.0 = set_pin_mode(w.0, 8, 0b0001);
        w.0 = set_pin_mode(w.0, 9, 0b0100);
        w.0 = set_pin_mode(w.0, 10, 0b1001);
        w.0 = set_pin_mode(w.0, 11, 0b0100);
        w.0 = set_pin_mode(w.0, 12, 0b0001);
        w.0 = set_pin_mode(w.0, 13, 0b1001);
        w.0 = set_pin_mode(w.0, 14, 0b1000);
        w.0 = set_pin_mode(w.0, 15, 0b1001);
    });
    pac::GPIOC.cr(0).modify(|w| {
        w.0 = set_pin_mode(w.0, 0, 0b0000);
        w.0 = set_pin_mode(w.0, 1, 0b0000);
        w.0 = set_pin_mode(w.0, 2, 0b0000);
        w.0 = set_pin_mode(w.0, 3, 0b0000);
        w.0 = set_pin_mode(w.0, 4, 0b0000);
        w.0 = set_pin_mode(w.0, 5, 0b0000);
        w.0 = set_pin_mode(w.0, 7, 0b0001);
    });
    pac::GPIOC.cr(1).modify(|w| {
        for pin in 8..16 {
            w.0 = set_pin_mode(w.0, pin, 0b0001);
        }
    });
    pac::GPIOD
        .cr(0)
        .modify(|w| w.0 = set_pin_mode(w.0, 2, 0b0001));

    pac::GPIOB.brr().write_value(pac::gpio::regs::Brr(0xffff));
    pac::GPIOB
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr((1 << 14) | (1 << 8)));
    pac::GPIOC
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr((0xffff << 16) | 0x1c80));
    pac::GPIOD
        .bsrr()
        .write_value(pac::gpio::regs::Bsrr((0xffff << 16) | (1 << 2)));

    pac::SPI2.cr1().write_value(pac::spi::regs::Cr1(
        SPI_CR1_MSTR
            | SPI_CR1_CPOL
            | SPI_CR1_CPHA
            | SPI_CR1_BR_0
            | SPI_CR1_BR_1
            | SPI_CR1_LSBFIRST
            | SPI_CR1_SSM
            | SPI_CR1_SSI
            | SPI_CR1_SPE,
    ));
    // Enable SPI2's DMA request lines once; individual transfers are
    // gated purely by each DMA channel's own EN bit (see
    // null_surface_phase/ledshift_phase).
    pac::SPI2
        .cr2()
        .write_value(pac::spi::regs::Cr2(SPI_CR2_RXDMAEN | SPI_CR2_TXDMAEN));
}

fn init_adc_hardware() {
    pac::RCC.ahbenr().modify(|w| w.set_dma1en(true));
    pac::RCC.apb2enr().modify(|w| w.set_adc1en(true));

    pac::ADC1
        .cr1()
        .write_value(pac::adc::regs::Cr1(ADC_CR1_SCAN));
    pac::ADC1
        .smpr1()
        .write_value(pac::adc::regs::Smpr1(0x00ff_ffff));
    pac::ADC1
        .smpr2()
        .write_value(pac::adc::regs::Smpr2(0xffff_ffff));
    pac::ADC1
        .sqr3()
        .write_value(pac::adc::regs::Sqr3(pack_sequence(&ADC_SEQUENCE[0..6])));
    pac::ADC1
        .sqr2()
        .write_value(pac::adc::regs::Sqr2(pack_sequence(&ADC_SEQUENCE[6..12])));
    pac::ADC1.sqr1().write_value(pac::adc::regs::Sqr1(
        (15 << 20) | pack_sequence(&ADC_SEQUENCE[12..16]),
    ));

    pac::DMA1
        .ch(0)
        .par()
        .write_value(pac::ADC1.dr().as_ptr() as u32);
    pac::DMA1.ch(0).cr().write_value(pac::bdma::regs::Cr(
        DMA_CCR_MINC | DMA_CCR_PSIZE_16 | DMA_CCR_MSIZE_16,
    ));

    pac::ADC1.cr2().write_value(pac::adc::regs::Cr2(
        ADC_CR2_ADON | ADC_CR2_EXTTRIG | ADC_CR2_EXTSEL_SWSTART | ADC_CR2_DMA,
    ));
    pac::ADC1.cr2().modify(|w| w.0 |= ADC_CR2_RSTCAL);
    while pac::ADC1.cr2().read().0 & ADC_CR2_RSTCAL != 0 {}
    pac::ADC1.cr2().modify(|w| w.0 |= ADC_CR2_CAL);
    while pac::ADC1.cr2().read().0 & ADC_CR2_CAL != 0 {}
}

fn start_adc_conversion() {
    pac::ADC1.sr().modify(|w| w.0 &= !ADC_SR_EOC);
    pac::ADC1
        .cr2()
        .modify(|w| w.0 |= ADC_CR2_ADON | ADC_CR2_EXTTRIG | ADC_CR2_EXTSEL_SWSTART);
    pac::ADC1.cr2().modify(|w| w.0 |= ADC_CR2_SWSTART);
}

fn pack_sequence(channels: &[u8]) -> u32 {
    let mut value = 0u32;
    for (index, &channel) in channels.iter().enumerate() {
        value |= (channel as u32) << (index * 5);
    }
    value
}

fn set_pin_mode(register: u32, pin: u8, mode: u32) -> u32 {
    let shift = ((pin % 8) as u32) * 4;
    (register & !(0xf << shift)) | (mode << shift)
}
