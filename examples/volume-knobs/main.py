#!/usr/bin/env python3
"""Hook plugin. The runner calls invoke(action, payload); this process never sees hardware ids."""

from __future__ import annotations

import json
import sys

from volume import apply_action, assign_image, dial_jpeg, note_ready, open_backend


def placed_image(action: str, payload: dict, backend: object) -> str | None:
    """Assignment icon, or the live level when master volume is on the dial."""
    if action == "master_volume" and _target_kind(payload) == "big_knob" and backend is not None:
        level = backend.get("output").level  # type: ignore[attr-defined]
        return dial_jpeg(level)
    return assign_image(action)


def _target_kind(payload: dict) -> str | None:
    target = payload.get("target")
    if isinstance(target, dict):
        kind = target.get("kind")
        if isinstance(kind, str):
            return kind
    return None


def emit(message: dict) -> None:
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def main() -> None:
    backend = open_backend()
    warned = False
    for line in sys.stdin:
        if not line.strip():
            continue
        message = json.loads(line)
        kind = message.get("type")
        if kind == "init":
            note_ready()
            emit({"v": 1, "type": "ready"})
            continue
        if kind != "invoke":
            emit(
                {
                    "v": 1,
                    "type": "result",
                    "id": message.get("id", ""),
                    "ok": False,
                    "error": "unknown message",
                }
            )
            continue
        action = message.get("action")
        payload = message.get("payload")
        phase = message.get("phase", "invoke")
        if not isinstance(action, str) or not isinstance(payload, dict):
            emit(
                {
                    "v": 1,
                    "type": "result",
                    "id": message.get("id", ""),
                    "ok": False,
                    "error": "invoke requires action and payload",
                }
            )
            continue
        if phase == "assign":
            result = {"v": 1, "type": "result", "id": message["id"], "ok": True}
            image = placed_image(action, payload, backend)
            if image:
                result["image"] = image
            emit(result)
            continue
        if phase != "invoke":
            emit(
                {
                    "v": 1,
                    "type": "result",
                    "id": message["id"],
                    "ok": False,
                    "error": "unknown phase",
                }
            )
            continue
        if action != "switch_context":
            if backend is None:
                if not warned:
                    print(
                        "volume backend is only implemented on macOS",
                        file=sys.stderr,
                    )
                    warned = True
            else:
                apply_action(backend, action, payload)
        result = {"v": 1, "type": "result", "id": message["id"], "ok": True}
        image = placed_image(action, payload, backend)
        if image and _target_kind(payload) == "big_knob":
            result["image"] = image
        emit(result)


if __name__ == "__main__":
    main()
