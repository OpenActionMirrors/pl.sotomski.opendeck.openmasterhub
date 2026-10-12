#!/usr/bin/env python3
"""Reference hook plugin. The supervisor speaks JSON lines on stdin and stdout."""

from __future__ import annotations

import json
import sys


def emit(message: dict) -> None:
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def main() -> None:
    for line in sys.stdin:
        if not line.strip():
            continue
        message = json.loads(line)
        kind = message.get("type")
        if kind == "init":
            emit({"v": 1, "type": "ready"})
        elif kind == "invoke":
            emit({"v": 1, "type": "result", "id": message["id"], "ok": True})
        else:
            emit(
                {
                    "v": 1,
                    "type": "result",
                    "id": message.get("id", ""),
                    "ok": False,
                    "error": "unknown message",
                }
            )


if __name__ == "__main__":
    main()
