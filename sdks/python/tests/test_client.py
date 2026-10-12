"""Client behavior against a local websocket server. No masterhubd process."""

from __future__ import annotations

import asyncio
import json
import unittest

from openmasterhub import MasterHubClient, MasterHubError

try:
    from websockets.asyncio.server import serve
except ImportError:  # websockets < 13
    from websockets import serve


class ClientTests(unittest.IsolatedAsyncioTestCase):
    async def test_hello_event_command_and_trigger(self) -> None:
        events: list[dict] = []

        async def handler(socket) -> None:
            await socket.send(
                json.dumps(
                    {"v": 1, "type": "hello", "daemon": "masterhubd", "usb": False}
                )
            )
            await socket.send(
                json.dumps(
                    {
                        "v": 1,
                        "type": "event",
                        "action": "knob_rotated",
                        "payload": {"control": 0},
                    }
                )
            )
            first = json.loads(await socket.recv())
            self.assertEqual(first["type"], "command")
            self.assertEqual(first["action"], "set_backlight")
            await socket.send(
                json.dumps({"v": 1, "type": "result", "id": first["id"], "ok": True})
            )
            second = json.loads(await socket.recv())
            await socket.send(
                json.dumps(
                    {
                        "v": 1,
                        "type": "error",
                        "id": second["id"],
                        "message": "no such control",
                    }
                )
            )
            third = json.loads(await socket.recv())
            self.assertEqual(third["type"], "trigger")
            await socket.send(
                json.dumps({"v": 1, "type": "result", "id": third["id"], "ok": True})
            )
            await socket.wait_closed()

        async with serve(handler, "127.0.0.1", 0) as server:
            port = server.sockets[0].getsockname()[1]
            client = MasterHubClient(f"ws://127.0.0.1:{port}")
            client.on_event(events.append)
            hello = await client.connect()
            self.assertEqual(hello["daemon"], "masterhubd")
            for _ in range(20):
                if events:
                    break
                await asyncio.sleep(0.01)
            self.assertEqual(events[0]["action"], "knob_rotated")
            result = await client.command(
                "set_backlight",
                {"module": {"model": "M003K", "id": "knobs"}, "brightness": 40},
            )
            self.assertTrue(result["ok"])
            with self.assertRaises(MasterHubError) as raised:
                await client.command(
                    "set_backlight",
                    {"module": {"model": "M003K", "id": "knobs"}, "brightness": 40},
                )
            self.assertIn("no such control", str(raised.exception))
            triggered = await client.trigger(
                "knob_rotated",
                {
                    "module": {"model": "M003K", "id": "knobs"},
                    "control": 0,
                    "direction": "cw",
                },
            )
            self.assertTrue(triggered["ok"])
            await client.close()


if __name__ == "__main__":
    unittest.main()
