#![no_std]
#![no_main]

//! Motors off. USB serial debug is ON by default.
//! Any key that is not a `T=...` line stops debug and prints a menu.
//! Button A: flash-log *count* on the matrix (0-9), not a 1-9 animation.

use core::fmt::Write;
use cortex_m_rt::entry;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::InputPin;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use microbit::{
    display::blocking::Display,
    hal::{
        clocks::Clocks,
        rtc::Rtc,
        twim::{self, Twim},
        uarte::{Baudrate, Parity},
        Timer,
    },
    pac::interrupt,
    Board,
};

#[path = "uart_irq.rs"]
mod uart_irq;
use microbit_minicar::clock::{
    format_mmddyyyy_hhmmss, format_msec6, WallClock, STAMP_LEN,
};
use microbit_minicar::led;
use microbit_minicar::log_store::{
    append, iter_valid, EventKind, LogError, LogRecord, Seq, PAGE_SIZE,
};
use microbit_minicar::motor;
use microbit_minicar::selftest;
use microbit_minicar::serial_ui::{Cmd, SerialUi};
use panic_halt as _;

const TICKS_PER_SEC: u32 = 8;
const LOG_LEN: usize = 2 * PAGE_SIZE;

fn log_storage() -> &'static mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(0x0007_E000 as *mut u8, LOG_LEN) }
}

fn persist(
    nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>,
    rec: &LogRecord,
) {
    let mut ram = [0xFFu8; LOG_LEN];
    let _ = nvmc.read(0, &mut ram);
    match append(&mut ram, rec) {
        Ok(_) => {}
        Err(LogError::Full) => {
            ram.fill(0xFF);
            let _ = append(&mut ram, rec);
        }
    }
    let _ = nvmc.erase(0, LOG_LEN as u32);
    let _ = nvmc.write(0, &ram);
}

fn dump_log(nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>) {
    let mut ram = [0u8; LOG_LEN];
    let _ = nvmc.read(0, &mut ram);
    let mut w = uart_irq::writer();
    let _ = write!(w, "LOG\r\n");
    for rec in iter_valid(&ram) {
        let _ = write!(
            w,
            "seq={} kind={:?} unix={} ticks={} x={} y={} z={}\r\n",
            rec.seq, rec.kind, rec.unix, rec.ticks, rec.x, rec.y, rec.z
        );
    }
    let _ = write!(w, "END\r\n");
}

fn glyph(c: u8) -> [u8; 5] {
    match c {
        b'0' => [0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        b'1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b01110],
        b'2' => [0b01110, 0b10001, 0b00010, 0b00100, 0b11111],
        b'3' => [0b11110, 0b00001, 0b01110, 0b00001, 0b11110],
        b'4' => [0b10010, 0b10010, 0b11111, 0b00010, 0b00010],
        b'5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b11110],
        b'6' => [0b01110, 0b10000, 0b11110, 0b10001, 0b01110],
        b'7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b00100],
        b'8' => [0b01110, 0b10001, 0b01110, 0b10001, 0b01110],
        b'9' => [0b01110, 0b10001, 0b01111, 0b00001, 0b01110],
        b':' => [0b00000, 0b00100, 0b00000, 0b00100, 0b00000],
        b'/' => [0b00001, 0b00010, 0b00100, 0b01000, 0b10000],
        b'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        b' ' => [0, 0, 0, 0, 0],
        _ => [0b11111, 0b10001, 0b10001, 0b10001, 0b11111],
    }
}

fn show_glyph<D: DelayNs>(display: &mut Display, delay: &mut D, c: u8, ms: u32) {
    let g = glyph(c);
    let mut frame = [[0u8; 5]; 5];
    for y in 0..5 {
        for x in 0..5 {
            frame[y][x] = u8::from((g[y] >> (4 - x)) & 1 == 1);
        }
    }
    display.show(delay, frame, ms);
}

fn scroll_frame<D: DelayNs>(display: &mut Display, delay: &mut D, text: &[u8], origin: usize) {
    let mut frame = [[0u8; 5]; 5];
    for x in 0..5 {
        let col = origin + x;
        let ch_i = col / 6;
        let bit = 4usize.saturating_sub(col % 6);
        if ch_i < text.len() && col % 6 < 5 {
            let g = glyph(text[ch_i]);
            for y in 0..5 {
                frame[y][x] = u8::from((g[y] >> bit) & 1 == 1);
            }
        }
    }
    display.show(delay, frame, 160);
}

fn menu() {
    let mut w = uart_irq::writer();
    let _ = write!(
        w,
        "\r\n---DBG OFF---\r\nMENU\r\n\
         T=<10-digit unix>  set UTC clock (then Enter)\r\n\
         1 status\r\n\
         2 dump flash log\r\n\
         3 start debug logging\r\n\
         4 run on-target tests\r\n\
         5 LED count 1-9 (display test)\r\n\
         6 show RTC (unix + msec)\r\n\
         7 clear RTC to 000000 msec\r\n\
         ? menu\r\n\
         (while debug ON, any key except T= stops debug and shows this menu)\r\n\
         (keys 1-7 and ? need no Enter)\r\n"
    );
}

fn run_tests() {
    let mut w = uart_irq::writer();
    let _ = write!(w, "SELFTEST\r\n");
    let r = selftest::run_all(&mut |name, ok| {
        let mark = if ok { "PASS" } else { "FAIL" };
        let _ = write!(w, "{mark} {name}\r\n");
    });
    let _ = write!(w, "RESULT pass={} fail={}\r\n", r.pass, r.fail);
}

fn log_count(nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>) -> u8 {
    let mut ram = [0u8; LOG_LEN];
    let _ = nvmc.read(0, &mut ram);
    iter_valid(&ram).count().min(9) as u8
}

#[entry]
fn main() -> ! {
    let board = Board::take().unwrap();
    let _clocks = Clocks::new(board.CLOCK).start_lfclk();
    let mut timer = Timer::new(board.TIMER0);
    let rtc = Rtc::new(board.RTC0, 4095).unwrap();
    rtc.enable_counter();

    let mut display = Display::new(board.display_pins);
    uart_irq::init(
        board.UARTE0,
        board.uart.into(),
        Baudrate::BAUD115200,
        Parity::EXCLUDED,
    );
    let mut nvmc = microbit::hal::nvmc::Nvmc::new(board.NVMC, log_storage());
    let mut i2c = Twim::new(
        board.TWIM0,
        board.i2c_external.into(),
        twim::Frequency::K100,
    );
    let mut button_a = board.buttons.button_a.into_floating_input();

    let _ = motor::stop(&mut i2c);
    let _ = led::disable(&mut i2c);

    let mut wall = WallClock::new_unset(TICKS_PER_SEC);
    let mut seq = Seq::new();
    let mut ui = SerialUi::new();
    let mut last_rx: u8 = 0;
    let mut last_dbg_ticks: u32 = 0;

    persist(
        &mut nvmc,
        &seq.emit(EventKind::Note, 0, rtc.get_counter(), 1, 0, 0),
    );
    let _ = write!(
        uart_irq::writer(),
        "boot clock_idle IRQ UART debug=ON  T=<unix> or any key for MENU\r\n"
    );

    loop {
        let ticks = rtc.get_counter();
        let mut n_rx = 0u32;
        while let Some(c) = uart_irq::read_byte() {
            n_rx += 1;
            last_rx = c;
            if c >= 32 && c < 127 {
                let _ = write!(uart_irq::writer(), "{}", c as char);
            }
            if let Some(cmd) = ui.push_byte(c) {
                apply_cmd(
                    cmd,
                    &mut display,
                    &mut timer,
                    &mut wall,
                    rtc.get_counter(),
                    &mut nvmc,
                    &mut seq,
                    last_rx,
                    ticks,
                    &ui,
                );
            }
        }

        if button_a.is_low().unwrap_or(false) {
            let n = log_count(&mut nvmc);
            if microbit_minicar::serial_ui::button_a_prints_log(ui.debug_on) {
                dump_log(&mut nvmc);
            }
            show_glyph(&mut display, &mut timer, b'0' + n, 600);
        }

        if ui.debug_on && n_rx == 0 {
            let dt = ticks.wrapping_sub(last_dbg_ticks);
            if dt >= 8 {
                last_dbg_ticks = ticks;
                let _ = write!(
                    uart_irq::writer(),
                    "dbg t={} set={} u={:?}\r\n",
                    ticks,
                    wall.is_set() as u8,
                    wall.unix_at(ticks),
                );
            }
        }

        if !ui.debug_on {
            if let Some(unix) = wall.unix_at(rtc.get_counter()) {
                let stamp = format_mmddyyyy_hhmmss(unix);
                let frames = stamp.len() * 6 + 5;
                for origin in 0..frames {
                    while let Some(c) = uart_irq::read_byte() {
                        last_rx = c;
                        if let Some(cmd) = ui.push_byte(c) {
                            apply_cmd(
                                cmd,
                                &mut display,
                                &mut timer,
                                &mut wall,
                                rtc.get_counter(),
                                &mut nvmc,
                                &mut seq,
                                last_rx,
                                rtc.get_counter(),
                                &ui,
                            );
                        }
                    }
                    scroll_frame(&mut display, &mut timer, &stamp, origin);
                }
                display.clear();
            } else {
                show_glyph(&mut display, &mut timer, b'T', 15);
            }
        }
    }
}

fn apply_cmd(
    cmd: Cmd,
    display: &mut Display,
    timer: &mut Timer<microbit::hal::pac::TIMER0>,
    wall: &mut WallClock,
    now_ticks: u32,
    nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>,
    seq: &mut Seq,
    last_rx: u8,
    ticks: u32,
    ui: &SerialUi,
) {
    let mut w = uart_irq::writer();
    match cmd {
        Cmd::SetTime(unix) => {
            wall.set(unix, now_ticks);
            persist(
                nvmc,
                &seq.emit(EventKind::ClockSet, unix, now_ticks, 0, 0, 0),
            );
            let _ = write!(w, "OK unix={} set={}\r\n", unix, wall.is_set());
        }
        Cmd::Status => {
            let _ = write!(
                w,
                "status ticks={} set={} unix={:?} rx_n={} last=0x{:02x} logs={} dbg={}\r\n",
                ticks,
                wall.is_set(),
                wall.unix_at(ticks),
                ui.rx_bytes,
                last_rx,
                log_count(nvmc),
                ui.debug_on as u8
            );
        }
        Cmd::Dump => dump_log(nvmc),
        Cmd::DebugOn => {
            let _ = write!(w, "debug ON\r\n");
        }
        Cmd::RunTests => run_tests(),
        Cmd::LedCount => {
            for d in b'1'..=b'9' {
                show_glyph(display, timer, d, 200);
            }
        }
        Cmd::ShowRtc => {
            let ms = wall.msec_since(now_ticks);
            let msec = format_msec6(ms);
            match wall.unix_at(now_ticks) {
                Some(u) => {
                    let stamp = format_mmddyyyy_hhmmss(u);
                    let _ = write!(
                        w,
                        "RTC unix={} stamp={} msec={}\r\n",
                        u,
                        core::str::from_utf8(&stamp).unwrap_or("?"),
                        core::str::from_utf8(&msec).unwrap_or("?")
                    );
                }
                None => {
                    let _ = write!(
                        w,
                        "RTC unset msec={}\r\n",
                        core::str::from_utf8(&msec).unwrap_or("?")
                    );
                }
            }
        }
        Cmd::ClearRtc => {
            wall.clear(now_ticks);
            let _ = write!(w, "RTC cleared msec=000000\r\n");
            show_glyph(display, timer, b'0', 400);
        }
        Cmd::Menu => menu(),
    }
}

#[interrupt]
fn UARTE0_UART0() {
    uart_irq::on_irq();
}
