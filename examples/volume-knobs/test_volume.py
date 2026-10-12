"""Knob step, clamp, and routed volume actions. No OS commands."""

from __future__ import annotations

import unittest

from volume import (
    RecordBackend,
    apply_action,
    _dial_ppm,
    assign_image,
    cached_is_current,
    level_to_scalar,
    scalar_to_level,
    step_level,
    website_command,
)


class StepTests(unittest.TestCase):
    def test_dial_picture_is_a_240_square(self) -> None:
        ppm = _dial_ppm(40)
        header = b"P6\n240 240\n255\n"
        self.assertTrue(ppm.startswith(header))
        self.assertEqual(len(ppm), len(header) + 240 * 240 * 3)

    def test_clockwise_and_counterclockwise_steps(self) -> None:
        self.assertEqual(step_level(50, "cw"), 55)
        self.assertEqual(step_level(50, "ccw"), 45)

    def test_clamp(self) -> None:
        self.assertEqual(step_level(98, "cw"), 100)
        self.assertEqual(step_level(2, "ccw"), 0)
        self.assertIsNone(step_level(50, "sideways"))

    def test_scalar_conversion_rounds_to_a_percent(self) -> None:
        self.assertEqual(scalar_to_level(0.3999999761581421), 40)
        self.assertEqual(scalar_to_level(0.5199999809265137), 52)
        self.assertEqual(scalar_to_level(1.0), 100)
        self.assertEqual(scalar_to_level(-0.2), 0)
        self.assertEqual(level_to_scalar(40), 0.4)
        self.assertEqual(level_to_scalar(150), 1.0)

    def test_cached_level_is_used_until_the_hold_expires(self) -> None:
        self.assertFalse(cached_is_current(None, 1.0, 2.0))
        self.assertTrue(cached_is_current(40, 1.0, 1.2))
        self.assertFalse(cached_is_current(40, 1.2, 1.2))


class ApplyTests(unittest.TestCase):
    def test_master_knob_steps_output(self) -> None:
        backend = _backend()
        apply_action(backend, "master_volume", _payload("knob", delta=1))
        self.assertEqual(backend.get("output").level, 55)

    def test_master_slider_sets_an_absolute_level(self) -> None:
        backend = _backend()
        apply_action(backend, "master_volume", _payload("slider", value=12))
        self.assertEqual(backend.get("output").level, 12)

    def test_mic_knob_steps_input(self) -> None:
        backend = _backend()
        apply_action(backend, "mic_volume", _payload("knob", delta=-1))
        self.assertEqual(backend.get("input").level, 45)

    def test_balance_steps_between_left_and_right(self) -> None:
        backend = _backend()
        apply_action(backend, "balance", _payload("knob", delta=1))
        self.assertEqual(backend.get_balance(), 5)
        apply_action(backend, "balance", _payload("roller", delta=-1))
        self.assertEqual(backend.get_balance(), 0)

    def test_assign_does_not_change_a_level(self) -> None:
        backend = _backend()
        apply_action(
            backend,
            "switch_context",
            {"assignment_id": "a1", "context_id": "main", "target": {"kind": "button", "page": 0, "index": 0}},
        )
        self.assertEqual(backend.get("output").level, 50)

    def test_mixer_icon_is_a_jpeg(self) -> None:
        image = assign_image("switch_context")
        self.assertIsNotNone(image)
        assert image is not None
        self.assertTrue(image.startswith("/9j/"))
        self.assertIsNone(assign_image("master_volume"))

    def test_volume_buttons_step_one_detent(self) -> None:
        backend = _backend()
        apply_action(backend, "master_volume_up", _press())
        self.assertEqual(backend.get("output").level, 55)
        apply_action(backend, "master_volume_down", _press())
        self.assertEqual(backend.get("output").level, 50)
        apply_action(backend, "mic_volume_up", _press())
        self.assertEqual(backend.get("input").level, 55)
        apply_action(backend, "mic_volume_down", _press())
        self.assertEqual(backend.get("input").level, 50)

    def test_a_button_release_does_not_change_volume(self) -> None:
        backend = _backend()
        apply_action(backend, "master_volume_up", _press(edge="release"))
        self.assertEqual(backend.get("output").level, 50)

    def test_mute_toggles_output(self) -> None:
        backend = _backend()
        apply_action(backend, "mute", _press())
        self.assertTrue(backend.get("output").muted)
        apply_action(backend, "mute", _press())
        self.assertFalse(backend.get("output").muted)

    def test_plus_and_mute_icons_are_jpegs(self) -> None:
        for action in ("master_volume_up", "master_volume_down", "mic_volume_up", "mute"):
            image = assign_image(action)
            self.assertIsNotNone(image)
            assert image is not None
            self.assertTrue(image.startswith("/9j/"))

    def test_open_website_builds_an_open_command_and_ignores_other_urls(self) -> None:
        self.assertEqual(
            website_command("chrome", "https://example.com/path"),
            ["open", "-a", "Google Chrome", "https://example.com/path"],
        )
        self.assertEqual(website_command("default", "http://localhost"), ["open", "http://localhost"])
        self.assertIsNone(website_command("default", "file:///etc/passwd"))
        self.assertIsNone(website_command("lynx", "https://example.com"))
        backend = _backend()
        apply_action(
            backend,
            "open_website",
            _press(params={"browser": "safari", "url": "https://example.com"}),
        )
        self.assertEqual(backend.get("output").level, 50)


def _backend() -> RecordBackend:
    backend = RecordBackend(":memory:")
    backend._write = lambda _payload: None  # type: ignore[method-assign]
    return backend


def _payload(kind: str, **fields: object) -> dict:
    payload: dict[str, object] = {
        "assignment_id": "a1",
        "context_id": "main",
        "target": {"kind": kind, "index": 0} if kind != "roller" else {"kind": kind},
    }
    payload.update(fields)
    return payload


def _press(**fields: object) -> dict:
    payload = _payload("button", edge="press")
    payload.update(fields)
    return payload


if __name__ == "__main__":
    unittest.main()
