import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer, type AddressInfo } from "node:net";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { ensureLocalDaemon, localDaemonListen, type EnsureDaemonOptions } from "./daemon.ts";

const NO_PATH: NodeJS.ProcessEnv = { PATH: "" };

test("a remote host is left alone", async () => {
  const launched: string[] = [];
  const result = await ensureLocalDaemon("ws://example.com:9123", {
    env: NO_PATH,
    launch: (binary) => launched.push(binary),
  });
  assert.equal(result.state, "remote");
  assert.deepEqual(launched, []);
});

test("localhost and ipv6 bind on this computer", () => {
  assert.deepEqual(localDaemonListen("ws://localhost:9123"), {
    checkHost: "127.0.0.1",
    port: 9123,
    address: "127.0.0.1:9123",
  });
  assert.deepEqual(localDaemonListen("ws://[::1]:9123"), {
    checkHost: "::1",
    port: 9123,
    address: "[::1]:9123",
  });
  assert.equal(localDaemonListen("ws://10.0.0.8:9123"), null);
  assert.equal(localDaemonListen("not a url"), null);
});

test("an open local port is not started again", async () => {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
  const address = server.address() as AddressInfo;
  try {
    const launched: string[] = [];
    const result = await ensureLocalDaemon(`ws://127.0.0.1:${address.port}`, {
      env: NO_PATH,
      launch: (binary) => launched.push(binary),
    });
    assert.equal(result.state, "listening");
    assert.deepEqual(launched, []);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});

test("a missing binary is reported and not launched", async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "masterhubd-missing-"));
  const port = await closedPort();
  try {
    const launched: string[] = [];
    const result = await ensureLocalDaemon(`ws://127.0.0.1:${port}`, {
      fromDir: dir,
      env: NO_PATH,
      launch: (binary) => launched.push(binary),
    });
    assert.equal(result.state, "missing");
    assert.deepEqual(launched, []);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("a binary beside the module is launched for localhost", async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "masterhubd-bin-"));
  const binary = path.join(dir, "masterhubd");
  const port = await closedPort();
  await writeFile(binary, "");
  await chmod(binary, 0o755);
  try {
    const launched: Array<{ binary: string; listen: string }> = [];
    const result = await ensureLocalDaemon(`ws://localhost:${port}`, launchOptions(dir, launched));
    assert.equal(result.state, "started");
    assert.deepEqual(launched, [{ binary, listen: `127.0.0.1:${port}` }]);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("MASTERHUBD_BIN wins over a sibling binary", async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "masterhubd-override-"));
  const sibling = path.join(dir, "masterhubd");
  const override = path.join(dir, "custom");
  const port = await closedPort();
  await writeFile(sibling, "");
  await writeFile(override, "");
  await chmod(sibling, 0o755);
  await chmod(override, 0o755);
  try {
    const launched: string[] = [];
    const result = await ensureLocalDaemon(`ws://127.0.0.1:${port}`, {
      fromDir: dir,
      env: { PATH: "", MASTERHUBD_BIN: override },
      launch: (binary) => launched.push(binary),
    });
    assert.equal(result.state, "started");
    assert.deepEqual(launched, [override]);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("a repo debug build is found by walking up from the plugin", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "masterhubd-repo-"));
  const binary = path.join(root, "daemons/target/debug/masterhubd");
  const fromDir = path.join(root, "plugins/companion-masterhub/dist");
  const port = await closedPort();
  await mkdir(path.dirname(binary), { recursive: true });
  await mkdir(fromDir, { recursive: true });
  await writeFile(binary, "");
  await chmod(binary, 0o755);
  try {
    const launched: string[] = [];
    const result = await ensureLocalDaemon(`ws://127.0.0.1:${port}`, {
      fromDir,
      env: NO_PATH,
      launch: (bin) => launched.push(bin),
    });
    assert.equal(result.state, "started");
    assert.deepEqual(launched, [binary]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

function launchOptions(
  fromDir: string,
  launched: Array<{ binary: string; listen: string }>,
): EnsureDaemonOptions {
  return {
    fromDir,
    env: NO_PATH,
    launch: (binary, listen) => launched.push({ binary, listen }),
  };
}

async function closedPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
  const address = server.address() as AddressInfo;
  await new Promise((resolve) => server.close(resolve));
  return address.port;
}
