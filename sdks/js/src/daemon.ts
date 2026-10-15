import { spawn } from "node:child_process";
import { constants } from "node:fs";
import { access, stat } from "node:fs/promises";
import net from "node:net";
import path from "node:path";

const LOCAL_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);
const WALK_LIMIT = 8;

export interface DaemonEnsureResult {
  state: "remote" | "listening" | "started" | "missing";
}

export interface EnsureDaemonOptions {
  /** Directory to search first. Plugins pass the directory of their bundled main file. */
  fromDir?: string;
  /** Environment used to resolve MASTERHUBD_BIN and PATH. Defaults to process.env. */
  env?: NodeJS.ProcessEnv;
  /** Replaces the detached process spawn. */
  launch?: (binary: string, listen: string) => void;
}

interface ListenTarget {
  checkHost: string;
  port: number;
  address: string;
}

/**
 * Start masterhubd when `endpoint` is on this computer and nothing is listening.
 * A remote host is left alone. The spawned process detaches, so closing the
 * caller does not stop the daemon. A second process exits 0 if the port is taken.
 */
export async function ensureLocalDaemon(
  endpoint: string,
  fromDirOrOptions?: string | EnsureDaemonOptions,
): Promise<DaemonEnsureResult> {
  const options: EnsureDaemonOptions =
    typeof fromDirOrOptions === "string" ? { fromDir: fromDirOrOptions } : (fromDirOrOptions ?? {});
  const target = localDaemonListen(endpoint);
  if (!target) {
    return { state: "remote" };
  }
  if (await portOpen(target.checkHost, target.port)) {
    return { state: "listening" };
  }
  const binary = await findDaemonBinary(searchDirs(options.fromDir), options.env ?? process.env);
  if (!binary) {
    return { state: "missing" };
  }
  (options.launch ?? launchDetached)(binary, target.address);
  return { state: "started" };
}

/** Local bind address for an endpoint, or null when this process should not start a daemon. */
export function localDaemonListen(endpoint: string): ListenTarget | null {
  let url: URL;
  try {
    url = new URL(endpoint);
  } catch {
    return null;
  }
  if (url.protocol !== "ws:" && url.protocol !== "wss:") {
    return null;
  }
  const hostname = url.hostname.replace(/^\[|\]$/g, "");
  if (!LOCAL_HOSTS.has(hostname)) {
    return null;
  }
  const port = Number(url.port || (url.protocol === "wss:" ? "443" : "80"));
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    return null;
  }
  const bindHost = hostname === "::1" ? "::1" : "127.0.0.1";
  const address = bindHost.includes(":") ? `[${bindHost}]:${port}` : `${bindHost}:${port}`;
  return { checkHost: bindHost, port, address };
}

function searchDirs(fromDir: string | undefined): string[] {
  if (fromDir) {
    return [fromDir];
  }
  return [import.meta.dirname];
}

async function findDaemonBinary(fromDirs: readonly string[], env: NodeJS.ProcessEnv): Promise<string | null> {
  const override = env.MASTERHUBD_BIN?.trim();
  if (override && (await isExecutable(override))) {
    return override;
  }
  const names = process.platform === "win32" ? ["masterhubd.exe", "masterhubd"] : ["masterhubd"];
  const built = [
    "daemons/target/release/masterhubd",
    "daemons/target/debug/masterhubd",
    "daemons/target/release/masterhubd.exe",
    "daemons/target/debug/masterhubd.exe",
  ];
  for (const start of fromDirs) {
    let dir = path.resolve(start);
    for (let depth = 0; depth < WALK_LIMIT; depth += 1) {
      for (const name of names) {
        const candidate = path.join(dir, name);
        if (await isExecutable(candidate)) {
          return candidate;
        }
      }
      for (const relative of built) {
        const candidate = path.join(dir, relative);
        if (await isExecutable(candidate)) {
          return candidate;
        }
      }
      const parent = path.dirname(dir);
      if (parent === dir) {
        break;
      }
      dir = parent;
    }
  }
  const entries = (env.PATH ?? "").split(path.delimiter).filter(Boolean);
  for (const dir of entries) {
    for (const name of names) {
      const candidate = path.join(dir, name);
      if (await isExecutable(candidate)) {
        return candidate;
      }
    }
  }
  return null;
}

async function isExecutable(file: string): Promise<boolean> {
  try {
    const info = await stat(file);
    if (!info.isFile()) {
      return false;
    }
    await access(file, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function portOpen(host: string, port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.connect({ host, port });
    let done = false;
    const finish = (open: boolean) => {
      if (done) {
        return;
      }
      done = true;
      socket.destroy();
      resolve(open);
    };
    socket.setTimeout(300, () => finish(false));
    socket.once("connect", () => finish(true));
    socket.once("error", () => finish(false));
  });
}

const LAUNCHER = [
  "const {spawn}=require('node:child_process');",
  "const child=spawn(process.env.MASTERHUBD_CHILD_BIN,[],{detached:true,stdio:'ignore',env:process.env});",
  "child.unref();",
].join("");

function launchDetached(binary: string, listen: string): void {
  const child = spawn(process.execPath, ["-e", LAUNCHER], {
    detached: true,
    stdio: "ignore",
    env: {
      ...process.env,
      MASTERHUBD_CHILD_BIN: binary,
      MASTERHUBD_LISTEN: listen,
    },
  });
  child.on("error", () => {
    // The caller retries the socket and reports that connection failure.
  });
  child.unref();
}
