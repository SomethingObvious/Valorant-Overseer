// Ink has no mouse API, so this is the whole of it: turn on the terminal's own
// mouse reporting, and read the escape sequences back out of the key handler.
//
// 1003 asks for motion as well as clicks, which is what makes hover possible.
// 1006 asks for the SGR encoding, which is the only one that survives past
// column 223 -- the older encoding packs coordinates into single bytes and
// simply cannot describe a wide terminal.
//
// Leaving a terminal in mouse-reporting mode makes selection and scrolling
// behave strangely afterwards, so disabling has to happen on every exit path.

const ENABLE = "[?1003h[?1006h";
const DISABLE = "[?1003l[?1006l";

export interface MouseEvent {
  /** 1-based, as the terminal reports it. */
  column: number;
  row: number;
  kind: "press" | "release" | "move" | "wheel-up" | "wheel-down";
}

let installed = false;

export function enableMouse(write: (data: string) => void): () => void {
  if (installed) return () => undefined;
  installed = true;
  write(ENABLE);

  const off = (): void => {
    if (!installed) return;
    installed = false;
    write(DISABLE);
  };

  // Every exit path, including the ones that skip React's cleanup.
  process.once("exit", off);
  process.once("SIGINT", off);
  process.once("SIGTERM", off);
  return off;
}

const SGR = /^\[<(\d+);(\d+);(\d+)([Mm])$/;

/**
 * Parses one SGR mouse report. Ink hands the sequence over with the escape
 * byte already stripped, so what arrives starts at "[<".
 */
/** A report anywhere in a read, rather than a read that is exactly one. */
const REPORT = /\[<(\d+);(\d+);(\d+)([Mm])/;

/** A report that has begun and not finished, left at the end of a read. */
const PARTIAL = /\[<?[\d;]*$/;

export interface Chunk {
  events: MouseEvent[];
  /** An unfinished report, to be put in front of the next read. */
  pending: string;
  /** True when this read was mouse traffic, so it must not reach the keys. */
  mouse: boolean;
}

/**
 * Every report in a read, and whatever is left over.
 *
 * Terminals coalesce: motion reporting sends one report per cell crossed, so
 * moving the pointer to a tab and clicking it arrives as several reports in a
 * single read, and can be cut in half at any byte. An anchored match sees none
 * of that as a mouse event, and the read then falls through to the key
 * handling, where the escape byte that starts every report reads as the Escape
 * key. Escape on the board quits, so moving the mouse closed the app.
 */
export function parseMouseChunk(pending: string, input: string): Chunk {
  const raw = pending + input;
  const events: MouseEvent[] = [];
  let rest = raw;
  for (;;) {
    const found = REPORT.exec(rest);
    if (!found) break;
    const one = parseMouse(found[0]);
    if (one) events.push(one);
    rest = rest.slice(found.index + found[0].length);
  }
  const partial = PARTIAL.exec(rest);
  const held = partial ? partial[0] : "";
  return {
    events,
    pending: held,
    mouse: events.length > 0 || held.length > 0 || raw !== input,
  };
}

export function parseMouse(input: string): MouseEvent | null {
  const m = SGR.exec(input);
  if (!m) return null;
  const button = Number(m[1]);
  const column = Number(m[2]);
  const row = Number(m[3]);
  const released = m[4] === "m";
  if (!Number.isFinite(button) || !Number.isFinite(column) || !Number.isFinite(row)) {
    return null;
  }

  // Bit 6 marks the wheel; bit 5 marks motion. The low two bits are the button.
  if (button & 64) {
    return { column, row, kind: (button & 1) === 0 ? "wheel-up" : "wheel-down" };
  }
  if (released) return { column, row, kind: "release" };
  if (button & 32) return { column, row, kind: "move" };
  return { column, row, kind: "press" };
}

/** A rectangle on the screen and what it stands for. */
export interface Hit<T> {
  top: number;
  height: number;
  left: number;
  width: number;
  value: T;
}

export function hitTest<T>(zones: Array<Hit<T>>, column: number, row: number): T | null {
  for (const z of zones) {
    if (row >= z.top && row < z.top + z.height && column >= z.left && column < z.left + z.width) {
      return z.value;
    }
  }
  return null;
}
