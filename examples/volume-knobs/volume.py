"""System master volume, microphone volume, and stereo balance."""

from __future__ import annotations

import base64
import ctypes
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol
from urllib.parse import urlparse

STEP = 5
BALANCE_STEP = 5
# Keep a level we just wrote until the hardware catches up, then read again.
CACHE_HOLD_S = 0.2


@dataclass
class ChannelState:
    level: int
    muted: bool


class VolumeBackend(Protocol):
    def get(self, channel: str) -> ChannelState: ...

    def set_level(self, channel: str, level: int) -> None: ...

    def set_muted(self, channel: str, muted: bool) -> None: ...

    def get_balance(self) -> int: ...

    def set_balance(self, balance: int) -> None: ...


def cached_is_current(level: int | None, now: float, idle_after: float) -> bool:
    """A cached level is current until the hold expires and the hardware can be read again."""
    return level is not None and now < idle_after


def scalar_to_level(scalar: float) -> int:
    """Core Audio reports a 0..1 scalar. Plugin levels are 0..100."""
    return max(0, min(100, int(round(float(scalar) * 100))))


def level_to_scalar(level: int) -> float:
    return max(0, min(100, int(level))) / 100.0


def step_level(level: int, direction: str, size: int = STEP) -> int | None:
    """Move `level` by one detent. Unknown directions are ignored."""
    if direction == "cw":
        delta = size
    elif direction == "ccw":
        delta = -size
    else:
        return None
    return max(0, min(100, level + delta))


_ICONS = {
    "switch_context": "mixer.jpg",
    "master_volume_up": "plus.jpg",
    "master_volume_down": "minus.jpg",
    "mic_volume_up": "plus.jpg",
    "mic_volume_down": "minus.jpg",
    "mute": "mute.jpg",
}

_BUTTON_STEPS = {
    "master_volume_up": ("output", "cw"),
    "master_volume_down": ("output", "ccw"),
    "mic_volume_up": ("input", "cw"),
    "mic_volume_down": ("input", "ccw"),
}

_BROWSERS = {
    "default": None,
    "safari": "Safari",
    "chrome": "Google Chrome",
    "firefox": "Firefox",
}


def assign_image(action: str) -> str | None:
    """Icon returned when an action is placed."""
    name = _ICONS.get(action)
    if name is None:
        return None
    path = Path(__file__).resolve().parent / "icons" / name
    if not path.is_file():
        return None
    return base64.b64encode(path.read_bytes()).decode("ascii")


_GLYPHS = {
    "0": ("11111", "10001", "10001", "10001", "10001", "10001", "11111"),
    "1": ("00100", "01100", "00100", "00100", "00100", "00100", "01110"),
    "2": ("11111", "00001", "00001", "11111", "10000", "10000", "11111"),
    "3": ("11111", "00001", "00001", "11111", "00001", "00001", "11111"),
    "4": ("10001", "10001", "10001", "11111", "00001", "00001", "00001"),
    "5": ("11111", "10000", "10000", "11111", "00001", "00001", "11111"),
    "6": ("11111", "10000", "10000", "11111", "10001", "10001", "11111"),
    "7": ("11111", "00001", "00001", "00010", "00100", "00100", "00100"),
    "8": ("11111", "10001", "10001", "11111", "10001", "10001", "11111"),
    "9": ("11111", "10001", "10001", "11111", "00001", "00001", "11111"),
}


def dial_jpeg(level: int) -> str | None:
    """240×240 JPEG of a volume level, for the dial panel."""
    level = max(0, min(100, int(level)))
    ppm = _dial_ppm(level)
    directory = tempfile.mkdtemp(prefix="openmasterhub-dial-")
    ppm_path = os.path.join(directory, "level.ppm")
    jpeg_path = os.path.join(directory, "level.jpg")
    try:
        with open(ppm_path, "wb") as handle:
            handle.write(ppm)
        result = subprocess.run(
            ["sips", "-s", "format", "jpeg", ppm_path, "--out", jpeg_path],
            capture_output=True,
            check=False,
        )
        if result.returncode != 0 or not os.path.isfile(jpeg_path):
            return None
        data = Path(jpeg_path).read_bytes()
    finally:
        for name in ("level.ppm", "level.jpg"):
            try:
                os.remove(os.path.join(directory, name))
            except OSError:
                pass
        try:
            os.rmdir(directory)
        except OSError:
            pass
    if not data.startswith(b"\xff\xd8"):
        return None
    return base64.b64encode(data).decode("ascii")


def _dial_ppm(level: int) -> bytes:
    size = 240
    pixels = bytearray([16, 18, 22]) * (size * size)

    def paint(x: int, y: int, color: tuple[int, int, int]) -> None:
        if 0 <= x < size and 0 <= y < size:
            start = (y * size + x) * 3
            pixels[start : start + 3] = bytes(color)

    text = str(level)
    scale = 8
    glyph_w = 5 * scale
    gap = 8
    text_w = len(text) * glyph_w + (len(text) - 1) * gap
    origin_x = (size - text_w) // 2
    origin_y = 58
    for index, char in enumerate(text):
        glyph = _GLYPHS.get(char)
        if glyph is None:
            continue
        left = origin_x + index * (glyph_w + gap)
        for row, bits in enumerate(glyph):
            for col, bit in enumerate(bits):
                if bit != "1":
                    continue
                for dy in range(scale):
                    for dx in range(scale):
                        paint(left + col * scale + dx, origin_y + row * scale + dy, (236, 238, 242))
    track_x, track_y, track_w, track_h = 28, 156, 184, 28
    for y in range(track_y, track_y + track_h):
        for x in range(track_x, track_x + track_w):
            paint(x, y, (42, 46, 54))
    fill = max(0, round(track_w * level / 100))
    for y in range(track_y, track_y + track_h):
        for x in range(track_x, track_x + fill):
            paint(x, y, (78, 168, 255))
    header = f"P6\n{size} {size}\n255\n".encode("ascii")
    return header + bytes(pixels)


def website_command(browser: str, url: str) -> list[str] | None:
    """macOS `open` argv for an http(s) URL, or None when the request should be ignored."""
    parsed = urlparse(url.strip())
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return None
    if browser not in _BROWSERS:
        return None
    app = _BROWSERS[browser]
    target = url.strip()
    if app is None:
        return ["open", target]
    return ["open", "-a", app, target]


def launch_website(params: object) -> None:
    """Open a website from assignment params. Tests that record volume do not launch."""
    if not isinstance(params, dict):
        return
    browser = params.get("browser", "default")
    url = params.get("url", "")
    if not isinstance(browser, str) or not isinstance(url, str):
        return
    command = website_command(browser, url)
    if command is None:
        return
    if sys.platform != "darwin":
        return
    subprocess.run(command, check=False)


def _is_press(payload: dict) -> bool:
    """Knob and slider payloads have no edge. A button release is ignored."""
    edge = payload.get("edge")
    return not isinstance(edge, str) or edge == "press"


def apply_action(backend: VolumeBackend, action: str, payload: dict) -> None:
    """Run one routed action. `payload` names an assignment and a control value."""
    if not isinstance(payload, dict) or action == "switch_context":
        return
    if action == "open_website":
        if _is_press(payload) and not isinstance(backend, RecordBackend):
            launch_website(payload.get("params"))
        return
    if not _is_press(payload):
        return
    if action == "mute":
        state = backend.get("output")
        backend.set_muted("output", not state.muted)
        return
    step = _BUTTON_STEPS.get(action)
    if step is not None:
        channel, direction = step
        updated = step_level(backend.get(channel).level, direction)
        if updated is not None:
            backend.set_level(channel, updated)
        return
    if action == "balance":
        _apply_balance(backend, payload)
        return
    channel = {"master_volume": "output", "mic_volume": "input"}.get(action)
    if channel is None:
        return
    target = payload.get("target")
    kind = target.get("kind") if isinstance(target, dict) else None
    if kind == "slider":
        value = payload.get("value")
        if isinstance(value, bool) or not isinstance(value, int):
            return
        backend.set_level(channel, max(0, min(100, value)))
        return
    delta = payload.get("delta")
    if isinstance(delta, bool) or not isinstance(delta, int) or delta == 0:
        return
    direction = "cw" if delta > 0 else "ccw"
    level = backend.get(channel).level
    for _ in range(abs(delta)):
        updated = step_level(level, direction)
        if updated is None:
            return
        level = updated
    backend.set_level(channel, level)


def _apply_balance(backend: VolumeBackend, payload: dict) -> None:
    delta = payload.get("delta")
    if isinstance(delta, bool) or not isinstance(delta, int) or delta == 0:
        return
    balance = backend.get_balance()
    step = BALANCE_STEP if delta > 0 else -BALANCE_STEP
    for _ in range(abs(delta)):
        balance = max(-100, min(100, balance + step))
    backend.set_balance(balance)


class RecordBackend:
    """In-memory levels that append each change as a JSON line. Used by tests."""

    def __init__(self, path: str) -> None:
        self.path = path
        self.levels = {"output": 50, "input": 50, "alert": 50}
        self.muted = {"output": False, "input": False, "alert": False}
        self.balance = 0

    def get(self, channel: str) -> ChannelState:
        return ChannelState(self.levels[channel], self.muted[channel])

    def set_level(self, channel: str, level: int) -> None:
        self.levels[channel] = level
        self._write({"channel": channel, "level": level})

    def set_muted(self, channel: str, muted: bool) -> None:
        self.muted[channel] = muted
        self._write({"channel": channel, "muted": muted})

    def get_balance(self) -> int:
        return self.balance

    def set_balance(self, balance: int) -> None:
        self.balance = balance
        self._write({"channel": "balance", "level": balance})

    def _write(self, payload: dict) -> None:
        with open(self.path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(payload) + "\n")


class OsascriptBackend:
    """macOS volume via `osascript`. Output mute is a real flag. Input and alert are levels."""

    def get(self, channel: str) -> ChannelState:
        settings = _settings()
        level = int(settings[f"{channel} volume"])
        muted = settings.get("output muted") == "true" if channel == "output" else False
        return ChannelState(level, muted)

    def set_level(self, channel: str, level: int) -> None:
        _run(f"set volume {channel} volume {int(level)}")

    def set_muted(self, channel: str, muted: bool) -> None:
        if channel != "output":
            return
        flag = "true" if muted else "false"
        _run(f"set volume output muted {flag}")

    def get_balance(self) -> int:
        return 0

    def set_balance(self, balance: int) -> None:
        return


class CoreAudioError(RuntimeError):
    pass


class _Address(ctypes.Structure):
    _fields_ = (
        ("mSelector", ctypes.c_uint32),
        ("mScope", ctypes.c_uint32),
        ("mElement", ctypes.c_uint32),
    )


def _fourcc(text: str) -> int:
    raw = text.encode("ascii")
    if len(raw) != 4:
        raise ValueError(text)
    return int.from_bytes(raw, "big")


class CoreAudioVolume:
    """Output and input volume. Alert volume is not a Core Audio device property."""

    _SYSTEM = 1
    _GLOBAL = _fourcc("glob")
    _VOLUME = _fourcc("vmvc")
    _MUTE = _fourcc("mute")
    _BALANCE = _fourcc("vbal")
    _DEVICES = {
        "output": (_fourcc("dOut"), _fourcc("outp")),
        "input": (_fourcc("dIn "), _fourcc("inpt")),
    }

    def __init__(self) -> None:
        self._lib = ctypes.CDLL("/System/Library/Frameworks/CoreAudio.framework/CoreAudio")
        self._lib.AudioObjectGetPropertyData.argtypes = [
            ctypes.c_uint32,
            ctypes.POINTER(_Address),
            ctypes.c_uint32,
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_uint32),
            ctypes.c_void_p,
        ]
        self._lib.AudioObjectGetPropertyData.restype = ctypes.c_int32
        self._lib.AudioObjectSetPropertyData.argtypes = [
            ctypes.c_uint32,
            ctypes.POINTER(_Address),
            ctypes.c_uint32,
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.c_void_p,
        ]
        self._lib.AudioObjectSetPropertyData.restype = ctypes.c_int32

    def get(self, channel: str) -> ChannelState:
        device, scope = self._target(channel)
        level = scalar_to_level(float(self._read(device, self._VOLUME, scope, ctypes.c_float)))
        muted = False
        if channel == "output":
            muted = int(self._read(device, self._MUTE, scope, ctypes.c_uint32)) != 0
        return ChannelState(level, muted)

    def set_level(self, channel: str, level: int) -> None:
        device, scope = self._target(channel)
        self._write(device, self._VOLUME, scope, ctypes.c_float(level_to_scalar(level)))
        # Setting output volume with osascript clears mute. A knob turn should too.
        if channel == "output":
            muted = int(self._read(device, self._MUTE, scope, ctypes.c_uint32)) != 0
            if muted:
                self._write(device, self._MUTE, scope, ctypes.c_uint32(0))

    def set_muted(self, channel: str, muted: bool) -> None:
        if channel != "output":
            return
        device, scope = self._target(channel)
        self._write(device, self._MUTE, scope, ctypes.c_uint32(1 if muted else 0))

    def get_balance(self) -> int:
        device, _scope = self._target("output")
        scalar = float(self._read(device, self._BALANCE, self._GLOBAL, ctypes.c_float))
        return max(-100, min(100, int(round(scalar * 100))))

    def set_balance(self, balance: int) -> None:
        device, _scope = self._target("output")
        scalar = max(-100, min(100, int(balance))) / 100.0
        self._write(device, self._BALANCE, self._GLOBAL, ctypes.c_float(scalar))

    def _target(self, channel: str) -> tuple[int, int]:
        try:
            selector, scope = self._DEVICES[channel]
        except KeyError as err:
            raise CoreAudioError(channel) from err
        device = int(self._read(self._SYSTEM, selector, self._GLOBAL, ctypes.c_uint32))
        if device == 0:
            raise CoreAudioError(f"no {channel} device")
        return device, scope

    def _read(
        self,
        obj: int,
        selector: int,
        scope: int,
        kind: type[ctypes._SimpleCData],
    ) -> int | float:
        address = _Address(selector, scope, 0)
        size = ctypes.c_uint32(ctypes.sizeof(kind))
        value = kind()
        status = self._lib.AudioObjectGetPropertyData(
            ctypes.c_uint32(obj),
            ctypes.byref(address),
            ctypes.c_uint32(0),
            None,
            ctypes.byref(size),
            ctypes.byref(value),
        )
        if status != 0:
            raise CoreAudioError(f"get {selector:#x} failed ({status})")
        return value.value

    def _write(self, obj: int, selector: int, scope: int, value: ctypes._SimpleCData) -> None:
        address = _Address(selector, scope, 0)
        status = self._lib.AudioObjectSetPropertyData(
            ctypes.c_uint32(obj),
            ctypes.byref(address),
            ctypes.c_uint32(0),
            None,
            ctypes.c_uint32(ctypes.sizeof(value)),
            ctypes.byref(value),
        )
        if status != 0:
            raise CoreAudioError(f"set {selector:#x} failed ({status})")


class MacBackend:
    """Output and input use Core Audio. Alert volume still goes through osascript.

    A system volume write takes longer than one detent. Writes run on a side
    thread and later detents replace a write that has not started, so the
    runner is not stuck behind one process call per click.
    """

    def __init__(self) -> None:
        self._audio = CoreAudioVolume()
        self._script = OsascriptBackend()
        self._levels: dict[str, int | None] = {"output": None, "input": None}
        self._idle_after: dict[str, float] = {}
        self._muted: bool | None = None
        self._balance: int | None = None
        self._balance_idle = 0.0
        self._pending_level: dict[str, int] = {}
        self._pending_mute: bool | None = None
        self._cv = threading.Condition()
        self._hal = threading.Lock()
        self._thread = threading.Thread(
            target=self._flush,
            name="openmasterhub-volume",
            daemon=True,
        )
        self._thread.start()

    def get(self, channel: str) -> ChannelState:
        if channel == "alert":
            return self._script.get(channel)
        now = time.monotonic()
        with self._cv:
            cached = self._cached(channel, now)
            if cached is not None:
                return cached
        try:
            with self._hal:
                state = self._audio.get(channel)
        except CoreAudioError:
            return self._script.get(channel)
        with self._cv:
            cached = self._cached(channel, time.monotonic())
            if cached is not None:
                return cached
            self._levels[channel] = state.level
            if channel == "output":
                self._muted = state.muted
            return state

    def set_level(self, channel: str, level: int) -> None:
        if channel == "alert":
            self._script.set_level(channel, level)
            return
        with self._cv:
            self._levels[channel] = level
            self._pending_level[channel] = level
            self._idle_after[channel] = time.monotonic() + CACHE_HOLD_S
            if channel == "output":
                # A level change clears mute, including one queued ahead of this write.
                self._muted = False
                self._pending_mute = False
            self._cv.notify()

    def get_balance(self) -> int:
        now = time.monotonic()
        if self._balance is not None and cached_is_current(self._balance, now, self._balance_idle):
            return self._balance
        try:
            with self._hal:
                value = self._audio.get_balance()
        except CoreAudioError:
            return 0 if self._balance is None else self._balance
        self._balance = value
        return value

    def set_balance(self, balance: int) -> None:
        self._balance = balance
        self._balance_idle = time.monotonic() + CACHE_HOLD_S
        try:
            with self._hal:
                self._audio.set_balance(balance)
        except CoreAudioError as err:
            print(f"volume balance: {err}", file=sys.stderr)

    def set_muted(self, channel: str, muted: bool) -> None:
        if channel != "output":
            return
        with self._cv:
            self._muted = muted
            self._pending_mute = muted
            self._idle_after["output"] = time.monotonic() + CACHE_HOLD_S
            self._cv.notify()

    def _cached(self, channel: str, now: float) -> ChannelState | None:
        level = self._levels.get(channel)
        if not cached_is_current(level, now, self._idle_after.get(channel, 0.0)):
            return None
        assert level is not None
        muted = bool(self._muted) if channel == "output" else False
        return ChannelState(level, muted)

    def _flush(self) -> None:
        while True:
            with self._cv:
                while not self._pending_level and self._pending_mute is None:
                    self._cv.wait()
                levels = dict(self._pending_level)
                self._pending_level.clear()
                mute = self._pending_mute
                self._pending_mute = None
            for channel, level in levels.items():
                self._write_level(channel, level)
            if mute is not None:
                self._write_mute(mute)

    def _write_level(self, channel: str, level: int) -> None:
        try:
            with self._hal:
                try:
                    self._audio.set_level(channel, level)
                except CoreAudioError:
                    self._script.set_level(channel, level)
        except Exception as err:
            print(f"volume {channel}: {err}", file=sys.stderr)
            return
        self._hold(channel)

    def _write_mute(self, muted: bool) -> None:
        try:
            with self._hal:
                try:
                    self._audio.set_muted("output", muted)
                except CoreAudioError:
                    self._script.set_muted("output", muted)
        except Exception as err:
            print(f"volume mute: {err}", file=sys.stderr)
            return
        self._hold("output")

    def _hold(self, channel: str) -> None:
        with self._cv:
            until = time.monotonic() + CACHE_HOLD_S
            if until > self._idle_after.get(channel, 0.0):
                self._idle_after[channel] = until


def open_backend() -> VolumeBackend | None:
    if os.environ.get("OPENMASTERHUB_VOLUME_BACKEND") == "record":
        path = os.environ.get("OPENMASTERHUB_VOLUME_RECORD")
        if not path:
            raise RuntimeError("OPENMASTERHUB_VOLUME_RECORD is required")
        return RecordBackend(path)
    if sys.platform == "darwin":
        return MacBackend()
    return None


def note_ready() -> None:
    path = os.environ.get("OPENMASTERHUB_VOLUME_RECORD")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps({"event": "ready"}) + "\n")


def _run(script: str) -> str:
    completed = subprocess.run(
        ["osascript", "-e", script],
        check=True,
        capture_output=True,
        text=True,
    )
    return completed.stdout.strip()


def _settings() -> dict[str, str]:
    parsed: dict[str, str] = {}
    for part in _run("get volume settings").split(","):
        key, _, value = part.strip().partition(":")
        parsed[key.strip()] = value.strip()
    return parsed
