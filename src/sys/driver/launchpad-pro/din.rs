// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_executor::Spawner;
use crate::app::MidiEvent;
use crate::sys::midi::MidiPort;
use heapless::spsc::{Consumer, Producer, Queue};
use static_cell::StaticCell;
use stm32_metapac as pac;

const MIDI_QUEUE_SIZE: usize = 257;
const MIDI_TX_MAX_LEN: usize = 256;
const MIDI_TX_QUEUE_SIZE: usize = 17;

pub struct MidiTxMessage {
    len: usize,
    data: [u8; MIDI_TX_MAX_LEN],
}

struct HandleSlot<T> {
    inner: UnsafeCell<Option<T>>,
}

unsafe impl<T> Sync for HandleSlot<T> {}

impl<T> HandleSlot<T> {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(None),
        }
    }

    fn is_empty(&self) -> bool {
        unsafe { (*self.inner.get()).is_none() }
    }

    fn init(&self, value: T) {
        unsafe {
            *self.inner.get() = Some(value);
        }
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        unsafe { (*self.inner.get()).as_mut().map(f) }
    }
}

static MIDI_QUEUE: StaticCell<Queue<MidiEvent, MIDI_QUEUE_SIZE>> = StaticCell::new();
static MIDI_PRODUCER: HandleSlot<Producer<'static, MidiEvent>> = HandleSlot::new();
static MIDI_CONSUMER: HandleSlot<Consumer<'static, MidiEvent>> = HandleSlot::new();
static MIDI_TX_QUEUE: StaticCell<Queue<MidiTxMessage, MIDI_TX_QUEUE_SIZE>> = StaticCell::new();
static MIDI_TX_PRODUCER: HandleSlot<Producer<'static, MidiTxMessage>> = HandleSlot::new();
static MIDI_TX_CONSUMER: HandleSlot<Consumer<'static, MidiTxMessage>> = HandleSlot::new();
static MIDI_CABLES: AtomicU8 = AtomicU8::new(0);

const PA8_MASK: u32 = 0xf << 0;
const PA9_MASK: u32 = 0xf << 4;
const INPUT_FLOATING: u32 = 0x4;

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct DinCableState {
    pub midi_in_connected: bool,
    pub midi_out_connected: bool,
}

pub fn init_event_queues() {
    if MIDI_PRODUCER.is_empty() {
        let midi_queue = MIDI_QUEUE.init(Queue::new());
        let (producer, consumer) = midi_queue.split();
        MIDI_PRODUCER.init(producer);
        MIDI_CONSUMER.init(consumer);
    }

    if MIDI_TX_PRODUCER.is_empty() {
        let midi_tx_queue = MIDI_TX_QUEUE.init(Queue::new());
        let (producer, consumer) = midi_tx_queue.split();
        MIDI_TX_PRODUCER.init(producer);
        MIDI_TX_CONSUMER.init(consumer);
    }
}

pub fn cable_state() -> DinCableState {
    let state = MIDI_CABLES.load(Ordering::Acquire);
    DinCableState {
        midi_in_connected: (state & 0x01) != 0,
        midi_out_connected: (state & 0x02) != 0,
    }
}

pub fn dequeue_midi_event() -> Option<MidiEvent> {
    MIDI_CONSUMER
        .with_mut(|consumer| consumer.dequeue())
        .flatten()
}

pub fn enqueue_tx_message(data: &[u8]) -> Result<(), ()> {
    if data.is_empty() || data.len() > MIDI_TX_MAX_LEN {
        return Err(());
    }

    let mut message = MidiTxMessage {
        len: data.len(),
        data: [0; MIDI_TX_MAX_LEN],
    };
    message.data[..data.len()].copy_from_slice(data);

    MIDI_TX_PRODUCER
        .with_mut(|producer| producer.enqueue(message))
        .ok_or(())?
        .map_err(|_| ())
}

pub fn ingest_rx_byte(parser: &mut MidiParser, byte: u8) {
    if let Some(event) = parser.push(byte) {
        let _ = MIDI_PRODUCER.with_mut(|producer| producer.enqueue(event));
    }
}

use embassy_stm32::usart::{self, Config, Uart};
use embassy_stm32::{Peri, bind_interrupts, peripherals};

bind_interrupts!(struct Irqs {
    USART3 => usart::InterruptHandler<peripherals::USART3>;
    DMA1_CHANNEL2 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH2>;
    DMA1_CHANNEL3 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH3>;
});

pub fn spawn(
    spawner: &Spawner,
    usart: Peri<'static, peripherals::USART3>,
    rx: Peri<'static, peripherals::PB11>,
    tx: Peri<'static, peripherals::PB10>,
    rx_dma: Peri<'static, peripherals::DMA1_CH3>,
    tx_dma: Peri<'static, peripherals::DMA1_CH2>,
) {
    let mut config = Config::default();
    config.baudrate = 31250;

    let uart = Uart::new(usart, rx, tx, tx_dma, rx_dma, Irqs, config).unwrap();
    let (tx_drv, rx_drv) = uart.split();

    init_cable_detect();
    update_cable_state();

    spawner.spawn(din_tx_task(tx_drv).expect("din_tx_task spawn"));
    spawner.spawn(din_rx_task(rx_drv).expect("din_rx_task spawn"));
    spawner.spawn(din_cable_detect_task().expect("din_cable_detect_task token"));
}

#[embassy_executor::task]
async fn din_tx_task(mut tx: usart::UartTx<'static, embassy_stm32::mode::Async>) {
    loop {
        while let Some(message) = MIDI_TX_CONSUMER
            .with_mut(|consumer| consumer.dequeue())
            .flatten()
        {
            let data = &message.data[..message.len];
            let _ = tx.write(data).await;
        }

        embassy_time::Timer::after_millis(1).await;
    }
}

#[embassy_executor::task]
async fn din_rx_task(mut rx: usart::UartRx<'static, embassy_stm32::mode::Async>) {
    let mut parser = MidiParser::new();
    let mut buf = [0u8; 1];

    loop {
        match rx.read(&mut buf).await {
            Ok(_) => {
                ingest_rx_byte(&mut parser, buf[0]);
            }
            Err(_) => {
                embassy_time::Timer::after_millis(1).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn din_cable_detect_task() {
    loop {
        update_cable_state();
        embassy_time::Timer::after_millis(10).await;
    }
}

pub struct MidiParser {
    status: u8,
    data: [u8; 2],
    len: usize,
}

impl MidiParser {
    pub const fn new() -> Self {
        Self {
            status: 0,
            data: [0; 2],
            len: 0,
        }
    }

    pub fn push(&mut self, byte: u8) -> Option<MidiEvent> {
        if byte & 0x80 != 0 {
            self.status = byte;
            self.len = 0;

            if short_message_len(byte) == Some(1) {
                return Some(MidiEvent {
                    port: MidiPort::Din,
                    status: byte,
                    data1: 0,
                    data2: 0,
                });
            }

            return None;
        }

        let needed = short_message_len(self.status)?;
        if needed == 1 {
            return None;
        }

        self.data[self.len] = byte;
        self.len += 1;

        if self.len + 1 < needed {
            return None;
        }

        let event = MidiEvent {
            port: MidiPort::Din,
            status: self.status,
            data1: self.data[0],
            data2: if needed > 2 { self.data[1] } else { 0 },
        };
        self.len = 0;
        Some(event)
    }
}

fn short_message_len(status: u8) -> Option<usize> {
    match status {
        0x80..=0x8f => Some(3),
        0x90..=0x9f => Some(3),
        0xa0..=0xaf => Some(3),
        0xb0..=0xbf => Some(3),
        0xc0..=0xcf => Some(2),
        0xd0..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf1 | 0xf3 => Some(2),
        0xf2 => Some(3),
        0xf6 | 0xf8..=0xff => Some(1),
        _ => None,
    }
}

fn init_cable_detect() {
    pac::RCC.apb2enr().modify(|w| w.set_gpioaen(true));
    pac::GPIOA.cr(1).modify(|w| {
        w.0 = (w.0 & !PA8_MASK) | (INPUT_FLOATING << 0);
        w.0 = (w.0 & !PA9_MASK) | (INPUT_FLOATING << 4);
    });
}

fn update_cable_state() {
    let idr = pac::GPIOA.idr().read().0;
    let midi_in_connected = (idr & (1 << 9)) == 0;
    let midi_out_connected = (idr & (1 << 8)) == 0;

    let state = (midi_in_connected as u8) | ((midi_out_connected as u8) << 1);
    MIDI_CABLES.store(state, Ordering::Release);
}
