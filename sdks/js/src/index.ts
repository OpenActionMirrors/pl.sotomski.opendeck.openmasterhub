import type {
  EventMessage,
  HelloMessage,
  InputAction,
  OutputCommand,
  ResultMessage,
} from "@openmasterhub/protocol";

type ProtocolVersion = typeof import("@openmasterhub/protocol").PROTOCOL_VERSION;

/** Matches the protocol package. Kept local so the built client has no runtime dependency. */
export const PROTOCOL_VERSION: ProtocolVersion = 1;

export type {
  ClientMessage,
  DaemonMessage,
  EventMessage,
  HelloMessage,
  InputAction,
  OutputCommand,
  ResultMessage,
} from "@openmasterhub/protocol";

export const SDK_VERSION = "0.2.0";

export { ensureLocalDaemon, type DaemonEnsureResult } from "./daemon.ts";

const DEFAULT_REQUEST_TIMEOUT_MS = 5000;

export class MasterHubError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "MasterHubError";
  }
}

export interface ClientOptions {
  /** WebSocket URL of masterhubd, for example ws://127.0.0.1:9123. */
  endpoint: string;
  /** How long a command or trigger waits for a reply. Defaults to 5 seconds. */
  requestTimeoutMs?: number;
}

interface PendingCall {
  resolve: (message: ResultMessage) => void;
  reject: (error: Error) => void;
}

interface HelloWait {
  promise: Promise<HelloMessage>;
  resolve: (message: HelloMessage) => void;
  reject: (error: Error) => void;
}

/**
 * Connection to masterhubd. Connecting receives broadcast events.
 * Call `connect` again after `close` or a dropped socket. This client does not retry.
 */
export class MasterHubClient {
  readonly endpoint: string;
  private readonly requestTimeoutMs: number;
  private socket: WebSocket | null = null;
  private readonly pending = new Map<string, PendingCall>();
  private readonly listeners: Array<(event: EventMessage) => void> = [];
  private readonly disconnectListeners: Array<(error: Error) => void> = [];
  private nextId = 1;
  private connectGeneration = 0;
  private hello: HelloWait | null = null;
  private sawHello = false;
  private closing = false;

  constructor(options: ClientOptions) {
    this.endpoint = options.endpoint;
    this.requestTimeoutMs = options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
  }

  /** Register before connect so events that arrive with the hello are kept. */
  onEvent(listener: (event: EventMessage) => void): () => void {
    this.listeners.push(listener);
    return () => {
      const index = this.listeners.indexOf(listener);
      if (index >= 0) {
        this.listeners.splice(index, 1);
      }
    };
  }

  /**
   * Called when an open connection drops. Not called for `close`, or when `connect` fails
   * before hello: that failure rejects the `connect` promise instead.
   */
  onDisconnect(listener: (error: Error) => void): () => void {
    this.disconnectListeners.push(listener);
    return () => {
      const index = this.disconnectListeners.indexOf(listener);
      if (index >= 0) {
        this.disconnectListeners.splice(index, 1);
      }
    };
  }

  connect(): Promise<HelloMessage> {
    this.connectGeneration += 1;
    const generation = this.connectGeneration;
    this.detachSocket();
    this.failCurrent(new MasterHubError("connection closed"));
    this.closing = false;
    this.sawHello = false;
    const hello = createHello();
    this.hello = hello;

    const socket = new WebSocket(this.endpoint);
    this.socket = socket;
    socket.addEventListener("message", (event) => {
      if (generation !== this.connectGeneration) {
        return;
      }
      if (typeof event.data !== "string") {
        this.failCurrent(new MasterHubError("binary frames are not supported"));
        this.detachSocket();
        return;
      }
      this.onMessage(event.data);
    });
    socket.addEventListener("error", () => {
      if (generation !== this.connectGeneration || this.sawHello) {
        return;
      }
      hello.reject(new MasterHubError("socket error"));
    });
    socket.addEventListener("close", () => {
      if (generation !== this.connectGeneration) {
        return;
      }
      const error = new MasterHubError("connection closed");
      const unexpected = this.sawHello && !this.closing;
      this.socket = null;
      this.failCurrent(error);
      if (unexpected) {
        this.emitDisconnect(error);
      }
    });
    return hello.promise;
  }

  command<A extends OutputCommand["action"]>(
    action: A,
    payload: Extract<OutputCommand, { action: A }>["payload"],
  ): Promise<ResultMessage> {
    return this.request("command", action, payload);
  }

  trigger<A extends InputAction["action"]>(
    action: A,
    payload: Extract<InputAction, { action: A }>["payload"],
  ): Promise<ResultMessage> {
    return this.request("trigger", action, payload);
  }

  close(): void {
    this.closing = true;
    this.connectGeneration += 1;
    this.detachSocket();
    this.failCurrent(new MasterHubError("connection closed"));
  }

  private request(
    kind: "command" | "trigger",
    action: string,
    payload: object,
  ): Promise<ResultMessage> {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return Promise.reject(new MasterHubError("not connected"));
    }
    const id = String(this.nextId);
    this.nextId += 1;
    return new Promise<ResultMessage>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (!this.pending.has(id)) {
          return;
        }
        this.pending.delete(id);
        reject(new MasterHubError("request timed out"));
      }, this.requestTimeoutMs);
      this.pending.set(id, {
        resolve: (message) => {
          clearTimeout(timer);
          resolve(message);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      socket.send(JSON.stringify({ v: PROTOCOL_VERSION, type: kind, id, action, payload }));
    });
  }

  private onMessage(raw: string): void {
    let message: {
      v?: number;
      type?: string;
      id?: string;
      ok?: boolean;
      message?: string;
      daemon?: string;
      usb?: unknown;
      modules?: unknown;
      sites?: unknown;
    };
    try {
      message = JSON.parse(raw) as typeof message;
    } catch {
      this.failCurrent(new MasterHubError("invalid json"));
      return;
    }
    if (!this.sawHello) {
      if (!isHello(message)) {
        this.hello?.reject(new MasterHubError("expected a masterhubd hello"));
        this.detachSocket();
        return;
      }
      this.sawHello = true;
      this.hello?.resolve(message);
      return;
    }
    if (message.type === "event") {
      for (const listener of this.listeners) {
        listener(message as EventMessage);
      }
      return;
    }
    if (message.type === "result" || message.type === "error") {
      const callId = message.id;
      if (!callId) {
        return;
      }
      const pending = this.pending.get(callId);
      if (!pending) {
        return;
      }
      this.pending.delete(callId);
      if (message.type === "result" && message.ok === true) {
        pending.resolve(message as ResultMessage);
      } else {
        pending.reject(new MasterHubError(message.message ?? "daemon error"));
      }
    }
  }

  private failCurrent(error: Error): void {
    this.hello?.reject(error);
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }

  private detachSocket(): void {
    const socket = this.socket;
    this.socket = null;
    socket?.close();
  }

  private emitDisconnect(error: Error): void {
    for (const listener of [...this.disconnectListeners]) {
      listener(error);
    }
  }
}

function createHello(): HelloWait {
  let resolveHello!: (message: HelloMessage) => void;
  let rejectHello!: (error: Error) => void;
  let settled = false;
  const promise = new Promise<HelloMessage>((resolve, reject) => {
    resolveHello = resolve;
    rejectHello = reject;
  });
  return {
    promise,
    resolve: (message) => {
      if (settled) {
        return;
      }
      settled = true;
      resolveHello(message);
    },
    reject: (error) => {
      if (settled) {
        return;
      }
      settled = true;
      rejectHello(error);
    },
  };
}

function isHello(message: {
  v?: number;
  type?: string;
  daemon?: string;
  usb?: unknown;
  modules?: unknown;
  sites?: unknown;
}): message is HelloMessage {
  return (
    message.type === "hello" &&
    message.v === 1 &&
    message.daemon === "masterhubd" &&
    typeof message.usb === "boolean" &&
    Array.isArray(message.modules) &&
    message.modules.every((item) => typeof item === "string") &&
    isStringRecord(message.sites)
  );
}

function isStringRecord(value: unknown): value is Record<string, string> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  return Object.values(value).every((item) => typeof item === "string");
}
