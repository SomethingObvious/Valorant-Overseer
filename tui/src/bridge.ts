import { readFile } from "node:fs/promises";
import path from "node:path";
import type { Board, ConnectionState } from "./types.js";

// Speaks the protocol in backend/ws_server.py. Node has had a global WebSocket
// since v22, so this needs no package at all. Riot is never contacted from
// here: Python owns every request, including the rate limiting, and this
// process only reads what the bridge broadcasts.

const PROTOCOL = 1;

/** What ws_server.py can send. Everything is optional because the socket is a
 * trust boundary: a malformed frame must read as a missing field, not a crash. */
interface Envelope {
  type?: string | undefined;
  data?: unknown;
  id?: number | undefined;
  ok?: boolean | undefined;
  error?: string | undefined;
  message?: string | undefined;
  code?: string | undefined;
}

export interface Handlers {
  onBoard: (board: Board) => void;
  onStatus: (state: ConnectionState, detail?: string) => void;
}

interface Credentials {
  wsPort: number;
  token: string;
}

export function bridgePath(root: string): string {
  return path.join(root, ".overseer", "bridge.json");
}

async function readCredentials(root: string): Promise<Credentials | null> {
  try {
    const raw = await readFile(bridgePath(root), "utf8");
    const info = JSON.parse(raw) as { wsPort?: unknown; token?: unknown };
    const wsPort = Number(info.wsPort);
    const token = String(info.token ?? "");
    if (!token || !Number.isInteger(wsPort) || wsPort <= 0) return null;
    return { wsPort, token };
  } catch {
    return null;
  }
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
  timer: NodeJS.Timeout;
}

export class Bridge {
  private socket: WebSocket | null = null;
  private retry = 0;
  private timer: NodeJS.Timeout | null = null;
  private stopped = false;
  private nextId = 1;
  private pending = new Map<number, Pending>();

  constructor(
    private readonly root: string,
    private readonly handlers: Handlers,
  ) {}

  start(): void {
    this.stopped = false;
    void this.open();
  }

  /**
   * Asks the backend for something the board does not carry -- a career, a
   * match, the encounter log. These are on demand, never on a timer: the
   * expensive Riot calls happen when someone opens a view, not while the
   * scoreboard sits there.
   */
  request<T>(name: string, params: Record<string, unknown> = {}, timeoutMs = 20000): Promise<T> {
    const socket = this.socket;
    if (socket?.readyState !== 1) {
      return Promise.reject(new Error("not connected"));
    }
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("timed out"));
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timer,
      });
      socket.send(JSON.stringify({ type: "request", request: name, params, id }));
    });
  }

  private settle(msg: Envelope): void {
    const id = Number(msg.id);
    const waiting = this.pending.get(id);
    if (!waiting) return;
    this.pending.delete(id);
    clearTimeout(waiting.timer);
    if (msg.ok) {
      waiting.resolve(msg.data);
    } else {
      waiting.reject(new Error(String(msg.error ?? "request failed")));
    }
  }

  private failPending(reason: string): void {
    for (const [, waiting] of this.pending) {
      clearTimeout(waiting.timer);
      waiting.reject(new Error(reason));
    }
    this.pending.clear();
  }

  stop(): void {
    this.stopped = true;
    this.failPending("stopped");
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    try {
      this.socket?.close();
    } catch {
      // Already closing; nothing to do.
    }
    this.socket = null;
  }

  private async open(): Promise<void> {
    if (this.stopped) return;
    this.handlers.onStatus(this.retry === 0 ? "connecting" : "lost");

    // The backend writes bridge.json once it is listening, so on a cold start
    // this file simply does not exist yet. That is a wait, not an error.
    const creds = await readCredentials(this.root);
    if (!creds) {
      this.handlers.onStatus("connecting", "waiting for the backend");
      this.later(1000);
      return;
    }

    let socket: WebSocket;
    try {
      socket = new WebSocket(`ws://127.0.0.1:${creds.wsPort}`);
    } catch {
      this.later();
      return;
    }
    this.socket = socket;

    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({ type: "auth", token: creds.token, protocol: PROTOCOL }));
    });

    socket.addEventListener("message", (event: MessageEvent) => {
      let msg: Envelope;
      try {
        msg = JSON.parse(String(event.data)) as Envelope;
      } catch {
        return;
      }
      switch (msg.type) {
        case "auth_ok":
          this.retry = 0;
          this.handlers.onStatus("live");
          break;
        case "auth_error":
          // A bad token or an unsupported protocol will not fix itself by
          // reconnecting. Say so once and stop hammering the bridge.
          this.stopped = true;
          this.handlers.onStatus("lost", String(msg.message ?? msg.code ?? "rejected"));
          try {
            socket.close();
          } catch {
            // Nothing useful to do if the close itself fails.
          }
          break;
        case "state":
          if (msg.data && typeof msg.data === "object") {
            this.handlers.onBoard(msg.data as Board);
          }
          break;
        case "response":
          this.settle(msg);
          break;
        case "ping":
          socket.send(JSON.stringify({ type: "pong" }));
          break;
        default:
          break;
      }
    });

    socket.addEventListener("close", () => {
      this.socket = null;
      // Anything still waiting will never be answered on this socket.
      this.failPending("bridge disconnected");
      if (!this.stopped) this.later();
    });

    socket.addEventListener("error", () => {
      try {
        socket.close();
      } catch {
        // The close handler above schedules the retry either way.
      }
    });
  }

  private later(wait?: number): void {
    if (this.stopped) return;
    this.retry += 1;
    const delay = wait ?? Math.min(1000 * 2 ** Math.min(this.retry, 4), 10000);
    this.timer = setTimeout(() => void this.open(), delay);
    // A pending reconnect must not be the reason the process refuses to exit.
    this.timer.unref?.();
  }
}
