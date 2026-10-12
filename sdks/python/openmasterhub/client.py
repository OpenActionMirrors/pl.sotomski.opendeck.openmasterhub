"""Async client for the masterhubd WebSocket API."""

from __future__ import annotations

import asyncio
import json
from collections.abc import Callable
from typing import Any

try:
    from websockets.asyncio.client import connect
except ImportError:  # websockets < 13
    from websockets import connect


class MasterHubError(Exception):
    """The daemon rejected a call, or the socket failed."""


class MasterHubClient:
    """Connection to `masterhubd`. Connecting is the event subscription."""

    def __init__(self, endpoint: str) -> None:
        self.endpoint = endpoint
        self._ws: Any = None
        self._reader: asyncio.Task[None] | None = None
        self._pending: dict[str, asyncio.Future[dict[str, Any]]] = {}
        self._events: list[Callable[[dict[str, Any]], None]] = []
        self._hello: asyncio.Future[dict[str, Any]] | None = None
        self._next_id = 0

    def on_event(self, callback: Callable[[dict[str, Any]], None]) -> None:
        """Register a listener. Register before `connect` so the first events are kept."""
        self._events.append(callback)

    async def connect(self) -> dict[str, Any]:
        loop = asyncio.get_running_loop()
        self._hello = loop.create_future()
        self._ws = await connect(self.endpoint, open_timeout=5)
        self._reader = asyncio.create_task(self._read_loop())
        try:
            hello = await asyncio.wait_for(self._hello, 5)
        except Exception:
            await self.close()
            raise
        if (
            hello.get("v") != 1
            or hello.get("type") != "hello"
            or hello.get("daemon") != "masterhubd"
        ):
            await self.close()
            raise MasterHubError("expected a masterhubd hello")
        return hello

    async def command(self, action: str, payload: dict[str, Any]) -> dict[str, Any]:
        return await self._request("command", action, payload)

    async def trigger(self, action: str, payload: dict[str, Any]) -> dict[str, Any]:
        return await self._request("trigger", action, payload)

    async def close(self) -> None:
        socket = self._ws
        self._ws = None
        if socket is not None:
            await socket.close()
        reader = self._reader
        self._reader = None
        if reader is not None:
            reader.cancel()
            try:
                await reader
            except asyncio.CancelledError:
                pass
        self._fail_pending(MasterHubError("connection closed"))

    async def _request(
        self, kind: str, action: str, payload: dict[str, Any]
    ) -> dict[str, Any]:
        if self._ws is None:
            raise MasterHubError("not connected")
        self._next_id += 1
        call_id = str(self._next_id)
        future: asyncio.Future[dict[str, Any]] = asyncio.get_running_loop().create_future()
        self._pending[call_id] = future
        await self._ws.send(
            json.dumps(
                {
                    "v": 1,
                    "type": kind,
                    "id": call_id,
                    "action": action,
                    "payload": payload,
                }
            )
        )
        try:
            return await asyncio.wait_for(future, 5)
        finally:
            self._pending.pop(call_id, None)

    async def _read_loop(self) -> None:
        assert self._ws is not None
        try:
            async for raw in self._ws:
                if isinstance(raw, bytes):
                    self._fail_pending(MasterHubError("binary frames are not supported"))
                    return
                self._handle(raw)
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            error = exc if isinstance(exc, MasterHubError) else MasterHubError(str(exc))
            self._fail_pending(error)

    def _handle(self, raw: str) -> None:
        try:
            message = json.loads(raw)
        except json.JSONDecodeError as exc:
            self._fail_pending(MasterHubError(f"invalid json: {exc}"))
            return
        if not isinstance(message, dict):
            self._fail_pending(MasterHubError("message must be an object"))
            return
        kind = message.get("type")
        if self._hello is not None and not self._hello.done() and kind != "hello":
            self._hello.set_exception(MasterHubError(f"unexpected message type {kind}"))
            return
        if kind == "hello":
            if self._hello is not None and not self._hello.done():
                self._hello.set_result(message)
            return
        if kind == "event":
            for callback in list(self._events):
                callback(message)
            return
        if kind in ("result", "error"):
            call_id = message.get("id")
            future = self._pending.get(call_id) if isinstance(call_id, str) else None
            if future is None or future.done():
                return
            if kind == "result" and message.get("ok") is True:
                future.set_result(message)
            else:
                future.set_exception(
                    MasterHubError(str(message.get("message", "daemon error")))
                )

    def _fail_pending(self, exc: BaseException) -> None:
        if self._hello is not None and not self._hello.done():
            self._hello.set_exception(exc)
        for future in self._pending.values():
            if not future.done():
                future.set_exception(exc)
