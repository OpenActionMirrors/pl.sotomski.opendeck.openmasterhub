import assert from "node:assert/strict";
import { after, test } from "node:test";
import { WebSocketServer } from "ws";

import { MasterHubClient, MasterHubError } from "./index.ts";

const HELLO = {
  v: 1,
  type: "hello",
  daemon: "masterhubd",
  usb: false,
  modules: [],
  sites: {},
};

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
    socket.send(JSON.stringify(HELLO));
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
  assert.deepEqual(hello.modules, []);
  assert.deepEqual(hello.sites, {});
  for (let attempt = 0; events.length === 0 && attempt < 20; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  assert.deepEqual(events, ["knob_rotated"]);
  const result = await client.command("set_backlight", {
    module: { model: "M4315", id: "keys" },
    brightness: 40,
  });
  assert.equal(result.ok, true);
  await assert.rejects(
    () =>
      client.command("set_backlight", {
        module: { model: "M4315", id: "keys" },
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
  if (false) {
    await client.command("set_led_color", {
      module: { model: "M003K", id: "knobs" },
      control: "all",
      // @ts-expect-error set_led_color takes r, g, and b, not brightness
      brightness: 1,
    });
  }
  client.close();
});

test("connects again after close", async () => {
  const server = new WebSocketServer({ host: "127.0.0.1", port: 0 });
  await new Promise((resolve) => server.on("listening", resolve));
  after(() => server.close());
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("test server has no port");
  }
  let connections = 0;
  server.on("connection", (socket) => {
    connections += 1;
    socket.send(
      JSON.stringify({
        ...HELLO,
        usb: true,
        modules: ["M4315"],
        sites: { M4315: "22" },
      }),
    );
  });

  const client = new MasterHubClient({ endpoint: `ws://127.0.0.1:${address.port}` });
  const first = await client.connect();
  assert.equal(first.usb, true);
  assert.deepEqual(first.modules, ["M4315"]);
  client.close();
  const second = await client.connect();
  assert.equal(second.daemon, "masterhubd");
  assert.equal(connections, 2);
  client.close();
});

test("times out a command with no reply", async () => {
  const server = new WebSocketServer({ host: "127.0.0.1", port: 0 });
  await new Promise((resolve) => server.on("listening", resolve));
  after(() => server.close());
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("test server has no port");
  }
  server.on("connection", (socket) => {
    socket.send(JSON.stringify(HELLO));
  });

  const client = new MasterHubClient({
    endpoint: `ws://127.0.0.1:${address.port}`,
    requestTimeoutMs: 30,
  });
  await client.connect();
  await assert.rejects(
    () =>
      client.command("set_backlight", {
        module: { model: "M4315", id: "keys" },
        brightness: 1,
      }),
    (error: unknown) => error instanceof MasterHubError && error.message === "request timed out",
  );
  client.close();
});

test("reports an unexpected disconnect and ignores close", async () => {
  const server = new WebSocketServer({ host: "127.0.0.1", port: 0 });
  await new Promise((resolve) => server.on("listening", resolve));
  after(() => server.close());
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("test server has no port");
  }
  let opened: { close: () => void } | undefined;
  server.on("connection", (socket) => {
    opened = socket;
    socket.send(JSON.stringify(HELLO));
  });

  const client = new MasterHubClient({ endpoint: `ws://127.0.0.1:${address.port}` });
  const dropped = new Promise<Error>((resolve) => {
    client.onDisconnect(resolve);
  });
  await client.connect();
  opened?.close();
  const error = await dropped;
  assert.ok(error instanceof MasterHubError);

  let called = false;
  client.onDisconnect(() => {
    called = true;
  });
  await client.connect();
  client.close();
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(called, false);
});
