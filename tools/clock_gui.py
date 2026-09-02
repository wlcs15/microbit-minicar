#!/usr/bin/env python3
"""Host helper for clock_idle on the micro:bit v2.

Does not assert DTR/RTS (those often reset the nRF via DAPLink, which is why
minicom/pyserial can look like 'no serial'). Close minicom first — one process
owns /dev/ttyACM1.

Flash log on the micro:bit is the source of truth for floor runs; this GUI is
only for set-time and dump while USB is plugged in.
"""
from __future__ import annotations

import glob
import time
import tkinter as tk
from tkinter import messagebox, ttk

import serial
from serial.tools import list_ports


def encode_set_time(unix: int) -> bytes:
    return f"T={unix}\r\n".encode()


def encode_menu() -> bytes:
    return b"?"


def encode_dump() -> bytes:
    # ? leaves debug/menu, then 2 dumps (works from debug-on or menu).
    return b"?2"


def encode_tests() -> bytes:
    return b"?4"


def encode_show_rtc() -> bytes:
    return b"?6"


def encode_clear_rtc() -> bytes:
    return b"?7"


def open_serial(port: str, baud: int = 115200) -> serial.Serial:
    s = serial.Serial()
    s.port = port
    s.baudrate = baud
    s.timeout = 0.3
    s.write_timeout = 1
    s.dsrdtr = False
    s.rtscts = False
    s.dtr = False
    s.rts = False
    s.open()
    # Some CDC stacks pulse DTR on open; clear it again.
    try:
        s.dtr = False
        s.rts = False
    except Exception:
        pass
    time.sleep(0.2)
    s.reset_input_buffer()
    return s


class ClockGui(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("HS1002 clock_idle")
        self.geometry("640x420")
        self.ser: serial.Serial | None = None

        top = ttk.Frame(self)
        top.pack(fill=tk.X, padx=8, pady=8)
        ttk.Label(top, text="Port").pack(side=tk.LEFT)
        self.port = ttk.Combobox(top, width=36, values=self._ports())
        self.port.pack(side=tk.LEFT, padx=6)
        if "/dev/ttyACM1" in self.port["values"]:
            self.port.set("/dev/ttyACM1")
        elif self.port["values"]:
            self.port.current(0)
        ttk.Button(top, text="Refresh", command=self._refresh).pack(side=tk.LEFT)
        ttk.Button(top, text="Open", command=self._open).pack(side=tk.LEFT, padx=4)
        ttk.Button(top, text="Close", command=self._close).pack(side=tk.LEFT)

        btns = ttk.Frame(self)
        btns.pack(fill=tk.X, padx=8)
        ttk.Button(btns, text="Set time from this PC (UTC)", command=self._set_time).pack(
            side=tk.LEFT
        )
        ttk.Button(btns, text="Menu (?)", command=self._menu).pack(side=tk.LEFT, padx=4)
        ttk.Button(btns, text="Run on-target tests (4)", command=self._tests).pack(
            side=tk.LEFT, padx=4
        )
        ttk.Button(btns, text="Dump flash log", command=self._dump).pack(side=tk.LEFT, padx=8)
        ttk.Button(btns, text="Show RTC (6)", command=self._show_rtc).pack(side=tk.LEFT, padx=4)
        ttk.Button(btns, text="Clear RTC (7)", command=self._clear_rtc).pack(side=tk.LEFT, padx=4)

        self.log = tk.Text(self, height=18)
        self.log.pack(fill=tk.BOTH, expand=True, padx=8, pady=8)
        self._note(
            "Close minicom first. DTR is off.\n"
            "After Open you should see dbg/boot lines. Any key (or Menu) stops debug.\n"
            "Set time still sends T=<unix>. Button A on the board shows log COUNT (1 = one record), not 1-9."
        )

    def _ports(self) -> list[str]:
        found = [p.device for p in list_ports.comports()]
        for extra in glob.glob("/dev/ttyACM*") + glob.glob("/dev/ttyUSB*"):
            if extra not in found:
                found.append(extra)
        return found

    def _refresh(self) -> None:
        self.port["values"] = self._ports()

    def _note(self, msg: str) -> None:
        self.log.insert(tk.END, msg + "\n")
        self.log.see(tk.END)

    def _open(self) -> None:
        self._close()
        port = self.port.get().strip()
        try:
            self.ser = open_serial(port)
        except Exception as e:
            messagebox.showerror("Serial", str(e))
            return
        self._note(f"opened {port} 115200 dtr=0 rts=0")
        self.after(200, self._poll)

    def _close(self) -> None:
        if self.ser is not None:
            try:
                self.ser.close()
            except Exception:
                pass
            self.ser = None
            self._note("closed")

    def _poll(self) -> None:
        if self.ser is None or not self.ser.is_open:
            return
        try:
            n = self.ser.in_waiting
            if n:
                data = self.ser.read(n)
                self._note(data.decode("utf-8", errors="replace"))
        except Exception as e:
            self._note(f"rx error {e}")
            self._close()
            return
        self.after(200, self._poll)

    def _set_time(self) -> None:
        if self.ser is None:
            messagebox.showinfo("Serial", "Open the port first")
            return
        unix = int(time.time())
        raw = encode_set_time(unix)
        self.ser.write(raw)
        self._note(f"sent {raw!r}  (UTC unix={unix})")

    def _dump(self) -> None:
        if self.ser is None:
            messagebox.showinfo("Serial", "Open the port first")
            return
        self.ser.write(encode_dump())
        self._note("sent ?2 (menu then dump)")

    def _menu(self) -> None:
        if self.ser is None:
            messagebox.showinfo("Serial", "Open the port first")
            return
        self.ser.write(encode_menu())
        self._note("sent ? (stop debug + menu)")

    def _tests(self) -> None:
        if self.ser is None:
            messagebox.showinfo("Serial", "Open the port first")
            return
        self.ser.write(encode_tests())
        self._note("sent ?4 (stop debug, then tests)")

    def _show_rtc(self) -> None:
        if self.ser is None:
            messagebox.showinfo("Serial", "Open the port first")
            return
        self.ser.write(encode_show_rtc())
        self._note("sent ?6 (show RTC)")

    def _clear_rtc(self) -> None:
        if self.ser is None:
            messagebox.showinfo("Serial", "Open the port first")
            return
        self.ser.write(encode_clear_rtc())
        self._note("sent ?7 (clear RTC)")


if __name__ == "__main__":
    ClockGui().mainloop()
