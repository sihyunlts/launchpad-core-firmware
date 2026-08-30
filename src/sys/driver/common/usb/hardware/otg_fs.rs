// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025-2026 Anthony Hofmeister

use core::cell::UnsafeCell;
use super::super::UsbDeviceConfig;
use super::super::control::{control, handle_setup_request, next_ep0_chunk, SetupAction};
use super::super::midi::{parse_usb_midi_packet, SysexReceiver};
use super::super::queues::queues;
use stm32_metapac as pac;

const EP_MAX_PACKET: usize = 64;

struct OtgFsState {
    sysex_rx: SysexReceiver,
}

impl OtgFsState {
    const fn new() -> Self {
        Self {
            sysex_rx: SysexReceiver::new(),
        }
    }
}

struct OtgFsStateCell(UnsafeCell<OtgFsState>);
unsafe impl Sync for OtgFsStateCell {}

static OTG_STATE: OtgFsStateCell = OtgFsStateCell(UnsafeCell::new(OtgFsState::new()));

pub fn init(cfg: &'static UsbDeviceConfig) {
    control().init(cfg);

    init_clocks_and_pins();

    let otg = pac::USB_OTG_FS;

    // Soft disconnect during setup
    otg.dctl().write(|w| w.set_sdis(true));

    // Wait for AHB master IDLE
    while !otg.grstctl().read().ahbidl() {}

    // Core soft reset
    otg.grstctl().write(|w| w.set_csrst(true));
    while otg.grstctl().read().csrst() {}

    // Force Device Mode, Embedded FS PHY, and turnaround time
    otg.gusbcfg().write(|w| {
        w.set_fdmod(true);
        w.set_physel(true);
        w.set_trdt(6);
    });
    while otg.gintsts().read().cmod() {}

    // Power on transceiver, disable VBUS sensing (NOVBUSSENS = 1)
    otg.gccfg_v1().modify(|w| {
        w.set_pwrdwn(true);
        w.set_novbussens(true);
        w.set_vbusasen(false);
        w.set_vbusbsen(false);
        w.set_sofouten(false);
    });

    // Override B-session valid (force VBUS high in device mode)
    otg.gotgctl().modify(|w| {
        w.set_bvaloen(true);
        w.set_bvaloval(true);
    });

    // Device speed: Full Speed Internal PHY
    otg.dcfg().write(|w| {
        w.set_pfivl(pac::otg::vals::Pfivl::FRAME_INTERVAL_80);
        w.set_dspd(pac::otg::vals::Dspd::FULL_SPEED_INTERNAL);
    });

    // Configure FIFO sizes (total depth 320 words):
    // RX FIFO: 128 words (512 bytes)
    otg.grxfsiz().write(|w| w.set_rxfd(128));
    // EP0 TX FIFO: 32 words (128 bytes), start at 128
    otg.dieptxf0().write(|w| {
        w.set_sa(128);
        w.set_fd(32);
    });
    // EP1 TX FIFO: 64 words (256 bytes), start at 160
    otg.dieptxf(0).write(|w| {
        w.set_sa(160);
        w.set_fd(64);
    });

    // Flush RX and all TX FIFOs
    otg.grstctl().write(|w| {
        w.set_rxfflsh(true);
        w.set_txfflsh(true);
        w.set_txfnum(0x10);
    });
    while otg.grstctl().read().rxfflsh() || otg.grstctl().read().txfflsh() {}

    // Configure EP0 endpoints
    reset_core();

    // Unmask endpoint transfer interrupts
    otg.diepmsk().write(|w| {
        w.set_xfrcm(true);
    });
    otg.doepmsk().write(|w| {
        w.set_xfrcm(true);
        w.set_stupm(true);
    });

    // Unmask device endpoint interrupts (EP0, EP1)
    otg.daintmsk().write(|w| {
        w.set_iepm(0x03);
        w.set_oepm(0x03);
    });

    // Clear all pending core interrupts
    otg.gintsts().write_value(pac::otg::regs::Gintsts(0xFFFF_FFFF));

    // Unmask core interrupts
    otg.gintmsk().write(|w| {
        w.set_usbrst(true);
        w.set_enumdnem(true);
        w.set_rxflvlm(true);
        w.set_iepint(true);
        w.set_oepint(true);
        w.set_usbsuspm(true);
        w.set_wuim(true);
    });

    // Global interrupt enable
    otg.gahbcfg().write(|w| {
        w.set_gint(true);
    });

    // Connect (clear soft disconnect)
    otg.dctl().modify(|w| w.set_sdis(false));

    unsafe {
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::OTG_FS);
    }
}

fn init_clocks_and_pins() {
    // Enable OTG_FS clock and reset peripheral
    pac::RCC.ahb2enr().modify(|w| w.set_usb_otg_fsen(true));
    pac::RCC.ahb2rstr().modify(|w| w.set_usb_otg_fsrst(true));
    for _ in 0..16 {
        cortex_m::asm::nop();
    }
    pac::RCC.ahb2rstr().modify(|w| w.set_usb_otg_fsrst(false));

    // Enable GPIOA clock
    pac::RCC.ahb1enr().modify(|w| w.set_gpioaen(true));

    // Configure PA11 (DM) and PA12 (DP) as Alternate Function 10 (AF10)
    pac::GPIOA.moder().modify(|w| {
        w.set_moder(11, pac::gpio::vals::Moder::ALTERNATE);
        w.set_moder(12, pac::gpio::vals::Moder::ALTERNATE);
    });
    pac::GPIOA.afr(1).modify(|w| {
        w.set_afr(3, 10); // PA11 -> index 3 in AFRH (pins 8..15)
        w.set_afr(4, 10); // PA12 -> index 4 in AFRH
    });
    pac::GPIOA.ospeedr().modify(|w| {
        w.set_ospeedr(11, pac::gpio::vals::Ospeedr::VERY_HIGH_SPEED);
        w.set_ospeedr(12, pac::gpio::vals::Ospeedr::VERY_HIGH_SPEED);
    });
    pac::GPIOA.pupdr().modify(|w| {
        w.set_pupdr(11, pac::gpio::vals::Pupdr::FLOATING);
        w.set_pupdr(12, pac::gpio::vals::Pupdr::FLOATING);
    });
}

pub fn poll() {
    let otg = pac::USB_OTG_FS;

    loop {
        let ints = otg.gintsts().read();
        if ints.0 == 0 {
            break;
        }

        // USB Reset
        if ints.usbrst() {
            otg.gintsts().write(|w| w.set_usbrst(true));
            reset_core();
            continue;
        }

        // Speed Enumeration Done
        if ints.enumdne() {
            otg.gintsts().write(|w| w.set_enumdne(true));
            otg.gusbcfg().modify(|w| w.set_trdt(6));
            continue;
        }

        // RX FIFO Non-Empty
        if ints.rxflvl() {
            service_rx_fifo();
        }

        // IN Endpoint Interrupt
        if ints.iepint() {
            service_in_endpoints();
        }

        // OUT Endpoint Interrupt
        if ints.oepint() {
            service_out_endpoints();
        }

        // Suspend / Wakeup
        if ints.usbsusp() {
            otg.gintsts().write(|w| w.set_usbsusp(true));
        }
        if ints.wkupint() {
            otg.gintsts().write(|w| w.set_wkupint(true));
        }

        // Break if no more core interrupts
        let after_ints = otg.gintsts().read();
        if (after_ints.0 & (1 << 4 | 1 << 12 | 1 << 18 | 1 << 19)) == 0 {
            break;
        }
    }

    pump_tx();
}

fn reset_core() {
    let otg = pac::USB_OTG_FS;
    let ctrl = control();
    let Some(cfg) = ctrl.cfg else {
        return;
    };
    ctrl.init(cfg);

    // Reset address
    otg.dcfg().modify(|w| w.set_dad(0));

    // EP0 IN (mpsiz 0 = 64 bytes)
    otg.diepctl(0).write(|w| {
        w.set_mpsiz(0);
        w.set_snak(true);
    });

    // EP0 OUT
    otg.doeptsiz(0).write(|w| {
        w.set_xfrsiz(64);
        w.set_rxdpid_stupcnt(3);
    });
    otg.doepctl(0).write(|w| {
        w.set_mpsiz(0);
        w.set_epena(true);
        w.set_cnak(true);
    });
}

fn service_rx_fifo() {
    let otg = pac::USB_OTG_FS;
    let state = unsafe { &mut *OTG_STATE.0.get() };
    let q = queues();

    while otg.gintsts().read().rxflvl() {
        let status = otg.grxstsp().read();
        let ep_num = status.epnum() as usize;
        let bcnt = status.bcnt() as usize;
        let pktsts = status.pktstsd();

        match pktsts {
            pac::otg::vals::Pktstsd::SETUP_DATA_RX => {
                let w0 = otg.fifo(0).read().0;
                let w1 = otg.fifo(0).read().0;
                let mut setup = [0u8; 8];
                setup[0..4].copy_from_slice(&w0.to_ne_bytes());
                setup[4..8].copy_from_slice(&w1.to_le_bytes());

                // Re-arm EP0 OUT for next setup
                otg.doeptsiz(0).modify(|w| {
                    w.set_rxdpid_stupcnt(3);
                });
                otg.doepctl(0).modify(|w| {
                    w.set_cnak(true);
                });

                process_setup(setup);
            }
            pac::otg::vals::Pktstsd::OUT_DATA_RX => {
                let mut buf = [0u8; EP_MAX_PACKET];
                let len = bcnt.min(EP_MAX_PACKET);
                let mut chunks = buf[..len].chunks_exact_mut(4);
                for chunk in &mut chunks {
                    let data = otg.fifo(0).read().0;
                    chunk.copy_from_slice(&data.to_ne_bytes());
                }
                let rem = chunks.into_remainder();
                if !rem.is_empty() {
                    let data = otg.fifo(0).read().0;
                    rem.copy_from_slice(&data.to_ne_bytes()[..rem.len()]);
                }

                if ep_num == 1 {
                    for chunk in buf[..len].chunks_exact(4) {
                        parse_usb_midi_packet(
                            chunk,
                            &mut state.sysex_rx,
                            &mut |event| q.midi_rx.push(event),
                            &mut |msg| q.sysex_rx.push(msg),
                        );
                    }
                }
            }
            pac::otg::vals::Pktstsd::OUT_DATA_DONE => {
                if ep_num == 1 {
                    otg.doeptsiz(1).write(|w| {
                        w.set_pktcnt(1);
                        w.set_xfrsiz(64);
                    });
                    otg.doepctl(1).modify(|w| {
                        w.set_cnak(true);
                        w.set_epena(true);
                    });
                } else if ep_num == 0 {
                    otg.doeptsiz(0).write(|w| {
                        w.set_xfrsiz(64);
                        w.set_rxdpid_stupcnt(3);
                    });
                    otg.doepctl(0).modify(|w| {
                        w.set_cnak(true);
                        w.set_epena(true);
                    });
                }
            }
            pac::otg::vals::Pktstsd::SETUP_DATA_DONE => {
                otg.doepint(0).write(|w| w.set_stup(true));
            }
            _ => {
                let words = (bcnt + 3) / 4;
                for _ in 0..words {
                    let _ = otg.fifo(0).read().0;
                }
            }
        }
    }
}

fn service_in_endpoints() {
    let otg = pac::USB_OTG_FS;
    let daint = otg.daint().read();

    // EP0 IN
    if daint.iepint() & 1 != 0 {
        let ep_ints = otg.diepint(0).read();
        otg.diepint(0).write_value(ep_ints);

        let ctrl = control();
        if ctrl.pending_address != 0 {
            otg.dcfg().modify(|w| w.set_dad(ctrl.pending_address));
            ctrl.pending_address = 0;
        } else if let Some((ptr, len)) = next_ep0_chunk(ctrl) {
            write_ep0_in(ptr, len);
        } else {
            // Re-arm EP0 OUT
            otg.doeptsiz(0).write(|w| {
                w.set_xfrsiz(64);
                w.set_rxdpid_stupcnt(3);
            });
            otg.doepctl(0).modify(|w| {
                w.set_cnak(true);
                w.set_epena(true);
            });
        }
    }

    // EP1 IN
    if daint.iepint() & 2 != 0 {
        let ep_ints = otg.diepint(1).read();
        otg.diepint(1).write_value(ep_ints);
        pump_tx();
    }
}

fn service_out_endpoints() {
    let otg = pac::USB_OTG_FS;
    let daint = otg.daint().read();

    if daint.oepint() & 1 != 0 {
        let ep_ints = otg.doepint(0).read();
        otg.doepint(0).write_value(ep_ints);
    }
    if daint.oepint() & 2 != 0 {
        let ep_ints = otg.doepint(1).read();
        otg.doepint(1).write_value(ep_ints);
    }
}

fn process_setup(setup: [u8; 8]) {
    let otg = pac::USB_OTG_FS;

    match handle_setup_request(setup) {
        SetupAction::SendPacket { data, len } => {
            write_ep0_in(data, len);
        }
        SetupAction::StatusIn => {
            let ctrl = control();
            if ctrl.pending_address != 0 {
                otg.dcfg().modify(|w| w.set_dad(ctrl.pending_address));
                ctrl.pending_address = 0;
            }
            write_ep0_in(core::ptr::null(), 0);
        }
        SetupAction::Stall => {
            otg.diepctl(0).modify(|w| w.set_stall(true));
            otg.doepctl(0).modify(|w| w.set_stall(true));
        }
        SetupAction::ConfigurationChanged(cfg) => {
            if cfg != 0 {
                // Configure EP1 IN (Bulk IN)
                otg.diepctl(1).write(|w| {
                    w.set_mpsiz(64);
                    w.set_eptyp(pac::otg::vals::Eptyp::BULK);
                    w.set_txfnum(1);
                    w.set_sd0pid_sevnfrm(true);
                    w.set_snak(true);
                    w.set_usbaep(true);
                });

                // Configure EP1 OUT (Bulk OUT)
                otg.doeptsiz(1).write(|w| {
                    w.set_pktcnt(1);
                    w.set_xfrsiz(64);
                });
                otg.doepctl(1).write(|w| {
                    w.set_mpsiz(64);
                    w.set_eptyp(pac::otg::vals::Eptyp::BULK);
                    w.set_sd0pid_sevnfrm(true);
                    w.set_cnak(true);
                    w.set_epena(true);
                    w.set_usbaep(true);
                });

                pump_tx();
            }
            write_ep0_in(core::ptr::null(), 0);
        }
    }
}

fn write_ep0_in(data: *const u8, len: usize) {
    let otg = pac::USB_OTG_FS;

    otg.dieptsiz(0).write(|w| {
        w.set_pktcnt(1);
        w.set_xfrsiz(len as _);
    });
    otg.diepctl(0).modify(|w| {
        w.set_cnak(true);
        w.set_epena(true);
    });

    if len > 0 && !data.is_null() {
        let slice = unsafe { core::slice::from_raw_parts(data, len) };
        let fifo = otg.fifo(0);
        let mut chunks = slice.chunks_exact(4);
        for chunk in &mut chunks {
            let val = u32::from_ne_bytes(chunk.try_into().unwrap());
            fifo.write_value(pac::otg::regs::Fifo(val));
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut tmp = [0u8; 4];
            tmp[..rem.len()].copy_from_slice(rem);
            let val = u32::from_ne_bytes(tmp);
            fifo.write_value(pac::otg::regs::Fifo(val));
        }
    }
}

pub fn pump_tx() {
    let otg = pac::USB_OTG_FS;
    let ctrl = control();

    if ctrl.configuration == 0 {
        return;
    }

    let diepctl = otg.diepctl(1).read();
    if !diepctl.usbaep() || diepctl.epena() {
        return;
    }

    let q = queues();
    let mut payload = [0u8; EP_MAX_PACKET];
    let mut count = 0;

    while count + 4 <= EP_MAX_PACKET {
        let Some(packet) = q.midi_tx.pop() else {
            break;
        };
        payload[count..count + 4].copy_from_slice(&packet.data);
        count += 4;
    }

    if count > 0 {
        otg.dieptsiz(1).write(|w| {
            w.set_pktcnt(1);
            w.set_xfrsiz(count as _);
        });
        otg.diepctl(1).modify(|w| {
            w.set_cnak(true);
            w.set_epena(true);
        });

        let fifo = otg.fifo(1);
        let slice = &payload[..count];
        let mut chunks = slice.chunks_exact(4);
        for chunk in &mut chunks {
            let val = u32::from_ne_bytes(chunk.try_into().unwrap());
            fifo.write_value(pac::otg::regs::Fifo(val));
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut tmp = [0u8; 4];
            tmp[..rem.len()].copy_from_slice(rem);
            let val = u32::from_ne_bytes(tmp);
            fifo.write_value(pac::otg::regs::Fifo(val));
        }
    }
}

#[unsafe(export_name = "OTG_FS")]
pub extern "C" fn otg_fs_handler() {
    poll();
}
