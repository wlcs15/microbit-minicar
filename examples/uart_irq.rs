//! Interrupt-driven UARTE0 with separate TX/RX rings (≥16 bytes each).
//! Main loop never polls the UART peripheral; it only reads/writes rings.

use core::cell::RefCell;
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicBool, Ordering};
use cortex_m::interrupt::{free, Mutex};
use microbit::hal::gpio::{Output, Pin, PushPull};
use microbit::hal::uarte::{Baudrate, Parity};
use microbit::pac::{self, UARTE0};
use microbit_minicar::ring::Ring;

const RX_N: usize = 64;
const TX_N: usize = 512;
const DMA_TX: usize = 16;

static RX: Mutex<RefCell<Ring<RX_N>>> = Mutex::new(RefCell::new(Ring::new()));
static TX: Mutex<RefCell<Ring<TX_N>>> = Mutex::new(RefCell::new(Ring::new()));
static UART: Mutex<RefCell<Option<UARTE0>>> = Mutex::new(RefCell::new(None));
static TX_BUSY: AtomicBool = AtomicBool::new(false);

#[repr(align(4))]
struct DmaRx([u8; 1]);
#[repr(align(4))]
struct DmaTx([u8; DMA_TX]);

static mut RX_DMA: DmaRx = DmaRx([0]);
static mut TX_DMA: DmaTx = DmaTx([0; DMA_TX]);

pub fn init(uarte: UARTE0, pins: microbit::hal::uarte::Pins, baud: Baudrate, parity: Parity) {
    if uarte.enable.read().bits() != 0 {
        uarte.tasks_stoptx.write(|w| unsafe { w.bits(1) });
        while uarte.events_txstopped.read().bits() == 0 {}
        uarte.enable.write(|w| w.enable().disabled());
    }

    uarte.psel.rxd.write(|w| {
        unsafe { w.bits(pins.rxd.psel_bits()) };
        w.connect().connected()
    });
    let mut txd: Pin<Output<PushPull>> = pins.txd;
    let _ = embedded_hal::digital::OutputPin::set_high(&mut txd);
    uarte.psel.txd.write(|w| {
        unsafe { w.bits(txd.psel_bits()) };
        w.connect().connected()
    });
    uarte.psel.cts.write(|w| w.connect().disconnected());
    uarte.psel.rts.write(|w| w.connect().disconnected());
    let _ = pins.cts;
    let _ = pins.rts;

    uarte.baudrate.write(|w| w.baudrate().variant(baud));
    uarte.config.write(|w| w.parity().variant(parity));
    uarte.enable.write(|w| w.enable().enabled());
    uarte.intenset.write(|w| w.endrx().set().endtx().set().error().set());

    free(|cs| {
        *UART.borrow(cs).borrow_mut() = Some(uarte);
    });
    start_rx();
    unsafe {
        pac::NVIC::unmask(pac::Interrupt::UARTE0_UART0);
    }
}

fn start_rx() {
    free(|cs| {
        if let Some(u) = UART.borrow(cs).borrow().as_ref() {
            u.events_endrx.write(|w| w);
            let ptr = unsafe { addr_of_mut!(RX_DMA.0) as *mut u8 as u32 };
            u.rxd.ptr.write(|w| unsafe { w.ptr().bits(ptr) });
            u.rxd.maxcnt.write(|w| unsafe { w.maxcnt().bits(1) });
            u.tasks_startrx.write(|w| unsafe { w.bits(1) });
        }
    });
}

fn kick_tx() {
    if TX_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let n = free(|cs| {
        let mut tx = TX.borrow(cs).borrow_mut();
        let dst = unsafe { &mut *addr_of_mut!(TX_DMA.0) };
        tx.pop_chunk(dst)
    });
    if n == 0 {
        TX_BUSY.store(false, Ordering::SeqCst);
        return;
    }
    free(|cs| {
        if let Some(u) = UART.borrow(cs).borrow().as_ref() {
            u.events_endtx.write(|w| w);
            let ptr = unsafe { addr_of!(TX_DMA.0) as *const u8 as u32 };
            u.txd.ptr.write(|w| unsafe { w.ptr().bits(ptr) });
            u.txd.maxcnt.write(|w| unsafe { w.maxcnt().bits(n as u16) });
            u.tasks_starttx.write(|w| unsafe { w.bits(1) });
        }
    });
}

pub fn write_bytes(data: &[u8]) -> usize {
    let n = free(|cs| TX.borrow(cs).borrow_mut().push_slice(data));
    kick_tx();
    n
}

pub fn write_str(s: &str) -> usize {
    write_bytes(s.as_bytes())
}

pub fn read_byte() -> Option<u8> {
    free(|cs| RX.borrow(cs).borrow_mut().pop())
}

pub fn on_irq() {
    let mut restart_rx = false;
    let mut restart_tx = false;
    free(|cs| {
        let uart = UART.borrow(cs).borrow();
        let Some(u) = uart.as_ref() else {
            return;
        };
        if u.events_endrx.read().bits() != 0 {
            u.events_endrx.write(|w| w);
            let b = unsafe { core::ptr::read_volatile(addr_of!(RX_DMA.0[0])) };
            let _ = RX.borrow(cs).borrow_mut().push(b);
            restart_rx = true;
        }
        if u.events_endtx.read().bits() != 0 {
            u.events_endtx.write(|w| w);
            TX_BUSY.store(false, Ordering::SeqCst);
            restart_tx = true;
        }
        if u.events_error.read().bits() != 0 {
            u.events_error.write(|w| w);
            let _ = u.errorsrc.read().bits();
            u.errorsrc.write(|w| unsafe { w.bits(0) });
            restart_rx = true;
        }
    });
    if restart_rx {
        start_rx();
    }
    if restart_tx {
        kick_tx();
    }
}

pub struct Writer;

impl core::fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        write_str(s);
        Ok(())
    }
}

pub fn writer() -> Writer {
    Writer
}
