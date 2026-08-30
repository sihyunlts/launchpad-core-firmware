// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::sync::atomic::{AtomicU32, Ordering};
pub use stm32_metapac as pac;
pub use stm32_metapac::Interrupt;

pub const APP_VECTOR_TABLE: u32 = 0x0800_3000;
pub const SYSCLK_HZ: u32 = 48_000_000;

pub const EVENT_1KHZ: u32 = 1 << 0;
pub const EVENT_200HZ: u32 = 1 << 1;
pub const EVENT_20HZ: u32 = 1 << 2;

static EVENTS: AtomicU32 = AtomicU32::new(0);
static TICK_DIV: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" {
    fn WWDG();
    fn PVD();
    fn TAMPER();
    fn RTC();
    fn FLASH();
    fn RCC();
    fn EXTI0();
    fn EXTI1();
    fn EXTI2();
    fn EXTI3();
    fn EXTI4();
    fn DMA1_CHANNEL1();
    fn DMA1_CHANNEL2();
    fn DMA1_CHANNEL3();
    fn DMA1_CHANNEL4();
    fn DMA1_CHANNEL5();
    fn DMA1_CHANNEL6();
    fn DMA1_CHANNEL7();
    fn ADC1_2();
    fn USB_HP_CAN_TX();
    fn USB_LP_CAN1_RX0();
    fn CAN_RX1();
    fn CAN_SCE();
    fn EXTI9_5();
    fn TIM1_BRK();
    fn TIM1_UP();
    fn TIM1_TRG_COM();
    fn TIM1_CC();
    fn TIM2();
    fn TIM3();
    fn TIM4();
}

type Handler = unsafe extern "C" fn();

#[unsafe(link_section = ".vector_table.interrupts")]
#[unsafe(no_mangle)]
pub static __INTERRUPTS: [Handler; 31] = [
    WWDG,
    PVD,
    TAMPER,
    RTC,
    FLASH,
    RCC,
    EXTI0,
    EXTI1,
    EXTI2,
    EXTI3,
    EXTI4,
    DMA1_CHANNEL1,
    DMA1_CHANNEL2,
    DMA1_CHANNEL3,
    DMA1_CHANNEL4,
    DMA1_CHANNEL5,
    DMA1_CHANNEL6,
    DMA1_CHANNEL7,
    ADC1_2,
    USB_HP_CAN_TX,
    USB_LP_CAN1_RX0,
    CAN_RX1,
    CAN_SCE,
    EXTI9_5,
    TIM1_BRK,
    TIM1_UP,
    TIM1_TRG_COM,
    TIM1_CC,
    TIM2,
    TIM3,
    TIM4,
];

pub fn init_clocks() {
    pac::FLASH.acr().write(|w| {
        w.set_latency(pac::flash::vals::Latency::WS1);
        w.set_prftbe(true);
    });

    pac::RCC.cr().modify(|w| w.set_hseon(true));
    while !pac::RCC.cr().read().hserdy() {}

    pac::RCC.cfgr().write(|w| {
        w.set_pllsrc(pac::rcc::vals::Pllsrc::HSI_DIV2);
        w.set_pllmul(pac::rcc::vals::Pllmul::MUL8);
        w.set_ppre1(pac::rcc::vals::Ppre::DIV2);
        w.set_ppre2(pac::rcc::vals::Ppre::DIV2);
        w.set_usbpre(pac::rcc::vals::Usbpre::DIV1);
    });

    pac::RCC.cr().modify(|w| w.set_pllon(true));
    while !pac::RCC.cr().read().pllrdy() {}

    pac::RCC.cfgr().modify(|w| w.set_sw(pac::rcc::vals::Sw::PLL1_P));
    while pac::RCC.cfgr().read().sws() != pac::rcc::vals::Sw::PLL1_P {}
}

pub fn init_gpio_clocks() {
    pac::RCC.apb2enr().modify(|w| {
        w.set_afioen(true);
        w.set_gpioaen(true);
        w.set_gpioben(true);
        w.set_gpiocen(true);
        w.set_gpioden(true);
    });
}

pub fn init_tick_timer() {
    pac::RCC.apb1enr().modify(|w| w.set_tim4en(true));
    pac::TIM4.cr1().write(|w| w.set_cen(false));
    pac::TIM4.psc().write(|w| *w = 48 - 1);
    pac::TIM4.arr().write(|w| *w = pac::timer::regs::ArrCore(1000 - 1));
    pac::TIM4.egr().write(|w| w.set_ug(true));
    pac::TIM4.sr().write(|w| w.set_uif(false));
    pac::TIM4.dier().write(|w| w.set_uie(true));

    unsafe {
        cortex_m::peripheral::NVIC::unmask(Interrupt::TIM4);
    }

    pac::TIM4.cr1().modify(|w| w.set_cen(true));
}

pub fn take_events(mask: u32) -> u32 {
    EVENTS.fetch_and(!mask, Ordering::AcqRel) & mask
}

#[unsafe(export_name = "TIM4")]
pub extern "C" fn tim4_handler() {
    if !pac::TIM4.sr().read().uif() {
        return;
    }

    let tick = TICK_DIV.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let mut events = EVENT_1KHZ;
    if tick % 5 == 0 {
        events |= EVENT_200HZ;
    }
    if tick % 50 == 0 {
        events |= EVENT_20HZ;
    }

    EVENTS.fetch_or(events, Ordering::Release);

    pac::TIM4.sr().write(|w| w.set_uif(false));
}
