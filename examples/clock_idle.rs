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
        uarte::{self, Baudrate, Parity, Uarte},
        Timer,
    },
    Board,
};
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

fn dump_log<T>(
    uart: &mut Uarte<T>,
    nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>,
) where
    T: uarte::Instance,
{
    let mut ram = [0u8; LOG_LEN];
    let _ = nvmc.read(0, &mut ram);
    let _ = write!(uart, "LOG\r\n");
    for rec in iter_valid(&ram) {
        let _ = write!(
            uart,
            "seq={} kind={:?} unix={} ticks={} x={} y={} z={}\r\n",
            rec.seq, rec.kind, rec.unix, rec.ticks, rec.x, rec.y, rec.z
        );
    }
    let _ = write!(uart, "END\r\n");
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

fn menu<T: uarte::Instance>(uart: &mut Uarte<T>) {
    // Marker so leftover host-side dbg is obviously cut off.
    let _ = write!(
        uart,
        "\r\n---DBG OFF---\r\nMENU\r\n\
         T=<unix>  set UTC clock\r\n\
         1 status\r\n\
         2 dump flash log\r\n\
         3 start debug logging\r\n\
         4 run on-target tests\r\n\
         5 LED count 1-9 (display test)\r\n\
         6 show RTC (unix + msec)\r\n\
         7 clear RTC to 000000 msec\r\n\
         ? menu\r\n\
         (while debug ON, any key except T= stops debug and shows this menu)\r\n\
         (T=<10-digit unix> then Enter; 1-7 and ? need no Enter)\r\n"
    );
}

fn run_tests<T: uarte::Instance>(uart: &mut Uarte<T>) {
    let _ = write!(uart, "SELFTEST\r\n");
    let r = selftest::run_all(&mut |name, ok| {
        let mark = if ok { "PASS" } else { "FAIL" };
        let _ = write!(uart, "{mark} {name}\r\n");
    });
    let _ = write!(uart, "RESULT pass={} fail={}\r\n", r.pass, r.fail);
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
    let mut uart_timer = Timer::new(board.TIMER1);
    let rtc = Rtc::new(board.RTC0, 4095).unwrap();
    rtc.enable_counter();

    let mut display = Display::new(board.display_pins);
    let mut uart = Uarte::new(
        board.UARTE0,
        board.uart.into(),
        Parity::EXCLUDED,
        Baudrate::BAUD115200,
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
    let mut dbg_div: u32 = 0;

    persist(
        &mut nvmc,
        &seq.emit(EventKind::Note, 0, rtc.get_counter(), 1, 0, 0),
    );
    let _ = write!(
        uart,
        "boot clock_idle debug=ON send T=<unix> or any other key for MENU\r\n"
    );

    loop {
        let _ = motor::stop(&mut i2c);
        let ticks = rtc.get_counter();

        let mut b = [0u8; 4];
        if uart.read_timeout(&mut b[..1], &mut uart_timer, 12_000).is_ok() {
            let c = b[0];
            last_rx = c;
            show_glyph(&mut display, &mut timer, b'U', 20);
            if c >= 32 && c < 127 {
                let _ = write!(uart, "{}\r\n", c as char);
            }
            let _ = write!(uart, "rx 0x{:02x}\r\n", c);
            if let Some(cmd) = ui.push_byte(c) {
                apply_cmd(
                    cmd,
                    &mut uart,
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
            dump_log(&mut uart, &mut nvmc);
            let n = log_count(&mut nvmc);
            let _ = write!(uart, "btnA log_count={}\r\n", n);
            show_glyph(&mut display, &mut timer, b'0' + n, 300);
        }

        if ui.debug_on {
            dbg_div = dbg_div.wrapping_add(1);
            if dbg_div % 24 == 0 {
                let _ = write!(
                    uart,
                    "dbg ticks={} set={} unix={:?} rx_n={} last=0x{:02x} dbg={}\r\n",
                    ticks,
                    wall.is_set() as u8,
                    wall.unix_at(ticks),
                    ui.rx_bytes,
                    last_rx,
                    ui.debug_on as u8
                );
            }
        }

        if let Some(unix) = wall.unix_at(rtc.get_counter()) {
            let stamp = format_mmddyyyy_hhmmss(unix);
            let frames = stamp.len() * 6 + 5;
            for origin in 0..frames {
                let mut b = [0u8; 4];
                if uart.read_timeout(&mut b[..1], &mut uart_timer, 8_000).is_ok() {
                    last_rx = b[0];
                    if let Some(cmd) = ui.push_byte(b[0]) {
                        apply_cmd(
                            cmd,
                            &mut uart,
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
                scroll_frame(&mut display, &mut timer, &stamp[..STAMP_LEN], origin);
            }
            display.clear();
        } else {
            show_glyph(&mut display, &mut timer, b'T', 15);
        }
    }
}

fn apply_cmd<T>(
    cmd: Cmd,
    uart: &mut Uarte<T>,
    display: &mut Display,
    timer: &mut Timer<microbit::hal::pac::TIMER0>,
    wall: &mut WallClock,
    now_ticks: u32,
    nvmc: &mut microbit::hal::nvmc::Nvmc<microbit::pac::NVMC>,
    seq: &mut Seq,
    last_rx: u8,
    ticks: u32,
    ui: &SerialUi,
) where
    T: uarte::Instance,
{
    match cmd {
        Cmd::SetTime(unix) => {
            wall.set(unix, now_ticks);
            persist(
                nvmc,
                &seq.emit(EventKind::ClockSet, unix, now_ticks, 0, 0, 0),
            );
            let _ = write!(uart, "OK unix={} set={}\r\n", unix, wall.is_set());
        }
        Cmd::Status => {
            let _ = write!(
                uart,
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
        Cmd::Dump => dump_log(uart, nvmc),
        Cmd::DebugOn => {
            let _ = write!(uart, "debug ON\r\n");
        }
        Cmd::RunTests => run_tests(uart),
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
                        uart,
                        "RTC unix={} stamp={} msec={}\r\n",
                        u,
                        core::str::from_utf8(&stamp).unwrap_or("?"),
                        core::str::from_utf8(&msec).unwrap_or("?")
                    );
                    for origin in 0..(stamp.len() * 6 + 5) {
                        scroll_frame(display, timer, &stamp[..STAMP_LEN], origin);
                    }
                }
                None => {
                    let _ = write!(
                        uart,
                        "RTC unset msec={}\r\n",
                        core::str::from_utf8(&msec).unwrap_or("?")
                    );
                    for origin in 0..(6 * 6 + 5) {
                        scroll_frame(display, timer, &msec, origin);
                    }
                }
            }
            display.clear();
        }
        Cmd::ClearRtc => {
            wall.clear(now_ticks);
            let _ = write!(uart, "RTC cleared msec=000000\r\n");
            for origin in 0..(6 * 6 + 5) {
                scroll_frame(display, timer, b"000000", origin);
            }
            display.clear();
        }
        Cmd::Menu => menu(uart),
    }
}
