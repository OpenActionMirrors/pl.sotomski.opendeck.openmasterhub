import assert from "node:assert/strict";
import { after, test } from "node:test";
import { WebSocketServer } from "ws";

import { MasterHubClient, MasterHubError } from "./index.ts";

test("receives hello and events, and correlates command results", async () => {
  const server = new WebSocketServer({ host: "127.0.0.1", port: 0 });
  await new Promise((resolve) => server.on("listening", resolve));
  after(() => server.close());
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("test server has no port");
  }

  const seen: string[] = [];
  server.on("connection", (socket) => {
    socket.send(JSON.stringify({ v: 1, type: "hello", daemon: "masterhubd", usb: false }));
    socket.send(
      JSON.stringify({
        v: 1,
        type: "event",
        action: "knob_rotated",
        payload: { control: 0 },
      }),
    );
    socket.on("message", (raw) => {
      const message = JSON.parse(String(raw)) as { type: string; id: string; action: string };
      seen.push(message.type);
      if (seen.length === 1) {
        assert.equal(message.action, "set_backlight");
        socket.send(JSON.stringify({ v: 1, type: "result", id: message.id, ok: true }));
        return;
      }
      if (seen.length === 2) {
        socket.send(
          JSON.stringify({
            v: 1,
            type: "error",
            id: message.id,
            message: "no such control",
          }),
        );
        return;
      }
      assert.equal(message.type, "trigger");
      socket.send(JSON.stringify({ v: 1, type: "result", id: message.id, ok: true }));
    });
  });

  const client = new MasterHubClient({ endpoint: `ws://127.0.0.1:${address.port}` });
  const events: string[] = [];
  client.onEvent((event) => events.push(event.action));
  const hello = await client.connect();
  assert.equal(hello.daemon, "masterhubd");
  for (let attempt = 0; events.length === 0 && attempt < 20; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.deepEqual(events, ["knob_rotated"]);
  const result = await client.command("set_backlight", {
    module: { model: "M003K", id: "knobs" },
    brightness: 40,
  });
  assert.equal(result.ok, true);
  await assert.rejects(
    () =>
      client.command("set_backlight", {
        module: { model: "M003K", id: "knobs" },
        brightness: 40,
      }),
    (error: unknown) => error instanceof MasterHubError && error.message.includes("no such control"),
  );
  const triggered = await client.trigger("knob_rotated", {
    module: { model: "M003K", id: "knobs" },
    control: 0,
    direction: "cw",
  });
  assert.equal(triggered.ok, true);
  client.close();
});
