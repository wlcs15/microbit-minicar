#!/usr/bin/env python3
"""GUI + firmware-protocol tests (PTY / in-process). No micro:bit required."""
from __future__ import annotations

import os
import pty
import random
import sys
import time
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import clock_gui as cg  # noqa: E402


class FirmwareUi:
    """Mirrors src/serial_ui.rs (debug on → any non-T key is menu)."""

    def __init__(self) -> None:
        self.debug_on = True
        self.line = bytearray()
        self.cmds: list = []

    def push(self, c: int):
        if c in (10, 13):
            if self.line:
                s = self.line.decode()
                self.line.clear()
                if (
                    s.startswith("T=")
                    and s[2:].isdigit()
                    and len(s[2:]) >= 10
                    and int(s[2:]) >= 1_000_000_000
                ):
                    self.cmds.append(("set", int(s[2:])))
                    return ("set", int(s[2:]))
                self.debug_on = False
                self.cmds.append(("menu", None))
                return ("menu", None)
            return None
        if not self.line:
            if self.debug_on and c != ord("T"):
                self.debug_on = False
                self.cmds.append(("menu", None))
                return ("menu", None)
            if c == ord("?"):
                self.debug_on = False
                self.cmds.append(("menu", None))
                return ("menu", None)
            digit = {
                ord("1"): "status",
                ord("2"): "dump",
                ord("3"): "debugon",
                ord("4"): "tests",
                ord("5"): "led",
                ord("6"): "showrtc",
                ord("7"): "clearrtc",
            }.get(c)
            if digit:
                if digit == "debugon":
                    self.debug_on = True
                self.cmds.append((digit, None))
                return (digit, None)
        if c == ord("T") or self.line:
            self.line.append(c)
        else:
            self.debug_on = False
            self.cmds.append(("menu", None))
            return ("menu", None)
        return None

    def push_bytes(self, data: bytes):
        last = None
        for c in data:
            r = self.push(c)
            if r:
                last = r
        return last


class GuiEncodeTests(unittest.TestCase):
    def test_set_time_bytes(self):
        self.assertEqual(cg.encode_set_time(1_700_000_000), b"T=1700000000\r\n")
        self.assertEqual(cg.encode_show_rtc(), b"?6")
        self.assertEqual(cg.encode_clear_rtc(), b"?7")

    def test_menu_is_question_only(self):
        self.assertEqual(cg.encode_menu(), b"?")

    def test_dump_stops_debug_then_dumps(self):
        self.assertEqual(cg.encode_dump(), b"?2")

    def test_tests_stop_debug_then_run(self):
        self.assertEqual(cg.encode_tests(), b"?4")

    def test_set_time_through_firmware_ui(self):
        fw = FirmwareUi()
        self.assertEqual(
            fw.push_bytes(cg.encode_set_time(1_700_000_000)), ("set", 1_700_000_000)
        )
        self.assertTrue(fw.debug_on)

    def test_any_key_during_debug_is_menu(self):
        for k in b"34x?12":
            fw = FirmwareUi()
            self.assertTrue(fw.debug_on)
            self.assertEqual(fw.push(k), ("menu", None))
            self.assertFalse(fw.debug_on)

    def test_three_from_menu_enables_debug_next_key_stops(self):
        fw = FirmwareUi()
        fw.push(ord("?"))
        self.assertEqual(fw.push(ord("3")), ("debugon", None))
        self.assertTrue(fw.debug_on)
        self.assertEqual(fw.push(ord("q")), ("menu", None))
        self.assertFalse(fw.debug_on)

    def test_show_and_clear_rtc_from_menu(self):
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(cg.encode_show_rtc()), ("showrtc", None))
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(cg.encode_clear_rtc()), ("clearrtc", None))
        fw = FirmwareUi()
        fw.debug_on = False
        self.assertEqual(fw.push(ord("6")), ("showrtc", None))
        self.assertEqual(fw.push(ord("7")), ("clearrtc", None))

    def test_gui_dump_and_tests_from_debug(self):
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(cg.encode_dump()), ("dump", None))
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(cg.encode_tests()), ("tests", None))

    def test_unhappy_bad_t(self):
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(b"T=\n"), ("menu", None))
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(b"T=nope\n"), ("menu", None))

    def test_gui_methods_write_expected_bytes(self):
        class Fake:
            def __init__(self):
                self.buf = b""

            def write(self, b):
                self.buf += b

        g = cg.ClockGui.__new__(cg.ClockGui)
        g.ser = Fake()
        g._note = lambda *_: None
        with mock.patch("clock_gui.time.time", return_value=1_700_000_007):
            g._set_time()
        self.assertEqual(g.ser.buf, b"T=1700000007\r\n")
        g.ser.buf = b""
        g._menu()
        self.assertEqual(g.ser.buf, b"?")
        g.ser.buf = b""
        g._tests()
        self.assertEqual(g.ser.buf, b"?4")
        g.ser.buf = b""
        g._dump()
        self.assertEqual(g.ser.buf, b"?2")
        g.ser.buf = b""
        g._show_rtc()
        self.assertEqual(g.ser.buf, b"?6")
        g.ser.buf = b""
        g._clear_rtc()
        self.assertEqual(g.ser.buf, b"?7")

    def test_open_serial_dtr_off(self):
        master, slave = pty.openpty()
        try:
            s = cg.open_serial(os.ttyname(slave))
            self.assertFalse(s.dtr)
            self.assertFalse(s.rts)
            s.close()
        finally:
            os.close(master)
            try:
                os.close(slave)
            except OSError:
                pass

    def test_semi_random_gui_encodings(self):
        rng = random.Random(3)
        for _ in range(50):
            fw = FirmwareUi()
            op = rng.choice(["t", "menu", "dump", "tests", "show", "clear", "x"])
            if op == "t":
                u = 1_700_000_000 + rng.randint(0, 99)
                self.assertEqual(fw.push_bytes(cg.encode_set_time(u)), ("set", u))
            elif op == "show":
                self.assertEqual(fw.push_bytes(cg.encode_show_rtc()), ("showrtc", None))
            elif op == "clear":
                self.assertEqual(fw.push_bytes(cg.encode_clear_rtc()), ("clearrtc", None))
            elif op == "menu":
                self.assertEqual(fw.push_bytes(cg.encode_menu()), ("menu", None))
            elif op == "dump":
                self.assertEqual(fw.push_bytes(cg.encode_dump()), ("dump", None))
            elif op == "tests":
                self.assertEqual(fw.push_bytes(cg.encode_tests()), ("tests", None))
            else:
                self.assertEqual(fw.push(ord("z")), ("menu", None))

    def test_pty_roundtrip(self):
        master, slave = pty.openpty()
        try:
            s = cg.open_serial(os.ttyname(slave))
            s.write(cg.encode_set_time(1_700_000_000))
            time.sleep(0.05)
            self.assertIn(b"T=1700000000", os.read(master, 80))
            s.close()
        finally:
            os.close(master)


    def test_burst_menu_then_command(self):
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(b"?6"), ("showrtc", None))
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(b"?7"), ("clearrtc", None))
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(b"?4"), ("tests", None))
        fw = FirmwareUi()
        self.assertEqual(fw.push_bytes(b"?3"), ("debugon", None))
        self.assertTrue(fw.debug_on)
        self.assertEqual(fw.push(ord("x")), ("menu", None))


if __name__ == "__main__":
    loader = unittest.TestLoader()
    names = loader.getTestCaseNames(GuiEncodeTests)
    failed = False
    for seed in range(8):
        order = names[:]
        random.Random(seed).shuffle(order)
        suite = unittest.TestSuite(GuiEncodeTests(n) for n in order)
        print(f"--- shuffle seed={seed} ---")
        r = unittest.TextTestRunner(verbosity=1).run(suite)
        if not r.wasSuccessful():
            failed = True
            print("FAIL order", order)
    sys.exit(1 if failed else 0)
