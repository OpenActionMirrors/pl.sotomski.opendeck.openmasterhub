import type {
  EventMessage,
  HelloMessage,
  InputAction,
  OutputCommand,
  ResultMessage,
} from "@openmasterhub/protocol";

export { PROTOCOL_VERSION } from "@openmasterhub/protocol";
export type { ClientMessage, DaemonMessage, EventMessage } from "@openmasterhub/protocol";

export const SDK_VERSION = "0.1.0";

export class MasterHubError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "MasterHubError";
  }
}

export interface ClientOptions {
  /** WebSocket URL of masterhubd, for example ws://127.0.0.1:9123. */
  endpoint: string;
}

type OutputAction = OutputCommand["action"];

interface PendingCall {
  resolve: (message: ResultMessage) => void;
  reject: (error: Error) => void;
}

/**
 * Connection to masterhubd. Connecting receives broadcast events.
 * The runner does not proxy this socket.
 */
export class MasterHubClient {
  readonly endpoint: string;
  private socket: WebSocket | null = null;
  private readonly pending = new Map<string, PendingCall>();
  private readonly listeners: Array<(event: EventMessage) => void> = [];
  private nextId = 1;
  private helloPromise: Promise<HelloMessage>;
  private resolveHello!: (message: HelloMessage) => void;
  private rejectHello!: (error: Error) => void;
  private sawHello = false;

  constructor(options: ClientOptions) {
    this.endpoint = options.endpoint;
    this.helloPromise = new Promise((resolve, reject) => {
      this.resolveHello = resolve;
      this.rejectHello = reject;
    });
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

  connect(): Promise<HelloMessage> {
    const socket = new WebSocket(this.endpoint);
    this.socket = socket;
    socket.addEventListener("message", (event) => {
      this.onMessage(String(event.data));
    });
    socket.addEventListener("error", () => {
      if (!this.sawHello) {
        this.rejectHello(new MasterHubError("socket error"));
      }
    });
    socket.addEventListener("close", () => {
      this.rejectPending(new MasterHubError("connection closed"));
    });
    return this.helloPromise;
  }

  command(action: OutputAction, payload: OutputCommand["payload"]): Promise<ResultMessage> {
    return this.request("command", action, payload);
  }

  trigger(
    action: InputAction["action"],
    payload: InputAction["payload"],
  ): Promise<ResultMessage> {
    return this.request("trigger", action, payload);
  }

  close(): void {
    this.socket?.close();
    this.socket = null;
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
    const promise = new Promise<ResultMessage>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    socket.send(JSON.stringify({ v: 1, type: kind, id, action, payload }));
    return promise;
  }

  private onMessage(raw: string): void {
    let message: {
      v?: number;
      type?: string;
      id?: string;
      ok?: boolean;
      message?: string;
      daemon?: string;
    };
    try {
      message = JSON.parse(raw) as typeof message;
    } catch {
      this.rejectPending(new MasterHubError("invalid json"));
      return;
    }
    if (!this.sawHello) {
      if (message.type !== "hello" || message.v !== 1 || message.daemon !== "masterhubd") {
        this.rejectHello(new MasterHubError("expected a masterhubd hello"));
        return;
      }
      this.sawHello = true;
      this.resolveHello(message as HelloMessage);
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

  private rejectPending(error: Error): void {
    if (!this.sawHello) {
      this.rejectHello(error);
    }
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}
