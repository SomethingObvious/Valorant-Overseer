import { EventEmitter } from "node:events";
import { render } from "ink";
import { App } from "./app.js";
import { CAREER, ENCOUNTERS, PERFORMANCE, RECAP } from "./fixtures.js";
import { boardLayout } from "./layout.js";
import { SAMPLE_BOARD } from "./sample.js";
import { VIEWS } from "./views.js";

// Dev only. Mounts the real app against a terminal that claims to support raw
// mode, pushes keys at it, and checks the screen actually changed. The stories
// cannot cover this: they render with a stdin that has no raw mode, which is
// the one path where input is deliberately switched off.
//
// It also measures how often the app repaints while idle. A Windows console is
// slow at large writes, and a frame per animation tick is what makes a terminal
// app feel unresponsive even when its key handling is perfectly fine.

class FakeStdout extends EventEmitter {
  isTTY = true;
  frames: string[] = [];
  constructor(
    public columns: number,
    public rows = 40,
  ) {
    super();
  }
  resize(columns: number, rows: number): void {
    this.columns = columns;
    this.rows = rows;
    this.emit("resize");
  }
  write(chunk: string): boolean {
    this.frames.push(chunk);
    return true;
  }
}

class TtyStdin extends EventEmitter {
  isTTY = true;
  raw = false;
  private queue: string[] = [];
  setRawMode(value: boolean): this {
    this.raw = value;
    return this;
  }
  setEncoding(): this {
    return this;
  }
  /** Ink 7 listens for "readable" and pulls with read(), not "data". */
  read(): string | null {
    return this.queue.shift() ?? null;
  }
  push(chunk: string): void {
    this.queue.push(chunk);
    this.emit("readable");
  }
  resume(): this {
    return this;
  }
  pause(): this {
    return this;
  }
  ref(): void {}
  unref(): void {}
}

// Must include the escape byte. Without it this also strips ordinary
// bracketed text, such as the "[,]" key hints in the footer.
const ANSI = new RegExp(`${String.fromCharCode(27)}\\[[0-9;?]*[A-Za-z]`, "g");
const ESC = "";

function clean(frame: string): string {
  return frame.replace(ANSI, "");
}

function where(frame: string): string {
  if (frame.includes("RANKED")) return "session";
  if (frame.includes("SCOREBOARD")) return "recap";
  if (frame.includes("Stored in .overseer")) return "settings";
  if (frame.includes("RECENT MATCHES")) return "career";
  if (frame.includes("accounts logged in total")) return "encounters";
  if (frame.includes("ALLIES")) return "board";
  return "unknown";
}

async function main(): Promise<void> {
  const stdout = new FakeStdout(150);
  const stdin = new TtyStdin();
  const debug = Boolean(process.env.KEYCHECK_DEBUG);
  const failures: string[] = [];

  const app = render(
    <App
      root={process.cwd()}
      preview={SAMPLE_BOARD}
      previewApi={{
        recap: RECAP,
        profile: CAREER,
        performance: PERFORMANCE,
        encounters: ENCOUNTERS,
      }}
    />,
    {
      stdout: stdout as never,
      stdin: stdin as never,
      exitOnCtrlC: false,
      patchConsole: false,
    },
  );

  let exited = false;
  void app
    .waitUntilExit()
    .then(() => {
      exited = true;
    })
    .catch(() => {
      exited = true;
    });

  const settle = (): Promise<void> => new Promise((r) => setTimeout(r, 150));
  // Ink's final write each cycle is cursor housekeeping, so take the last frame
  // that actually has a screen in it.
  const latest = (): string =>
    [...stdout.frames]
      .reverse()
      .map(clean)
      .find((f) => f.trim().length > 40) ?? "";

  const press = async (key: string, label: string, expect: string): Promise<void> => {
    stdin.push(key);
    await settle();
    const got = where(latest());
    if (debug) console.error(`  ${label.padEnd(6)} -> ${got}`);
    if (got !== expect) failures.push(`${label} gave "${got}", expected "${expect}"`);
  };

  await settle();

  if (debug) {
    console.error(
      `  raw=${stdin.raw} readable=${stdin.listenerCount("readable")} ` +
        `data=${stdin.listenerCount("data")}`,
    );
  }
  if (!stdin.raw) failures.push("raw mode was never requested, so no key can arrive");
  if (stdin.listenerCount("readable") === 0 && stdin.listenerCount("data") === 0) {
    failures.push("ink never subscribed to stdin");
  }
  if (where(latest()) !== "board") failures.push("the board did not render");

  await press("2", "2", "career");
  await press("3", "3", "session");
  await press("4", "4", "recap");
  await press("5", "5", "encounters");
  await press("1", "1", "board");
  await press(",", "comma", "settings");
  await press(ESC, "esc", "board");
  await press("2", "2", "career");
  await press(ESC, "esc", "board");

  const before = latest();
  stdin.push("j");
  await settle();
  const moved = latest();
  if (before === moved) failures.push("j did not move the selection");
  stdin.push("k");
  await settle();
  if (latest() === moved) failures.push("k did not move the selection");

  // The layout maths has to agree with what was actually drawn, or every
  // click lands on the wrong row. Check it against the frame rather than
  // trusting the arithmetic.
  const boardFrame = latest();
  const frameLines = boardFrame.split(String.fromCharCode(10));
  const blue = (SAMPLE_BOARD.teams?.Blue ?? []).map((p) => p.puuid ?? "");
  const red = (SAMPLE_BOARD.teams?.Red ?? []).map((p) => p.puuid ?? "");
  const computed = boardLayout({
    hasMeta: true,
    tabs: VIEWS.map((v) => ({ key: v.key, digit: v.digit, text: v.label })),
    teams: [{ puuids: blue }, { puuids: red }],
    width: 150,
  });
  const tabLine = frameLines[computed.tabRow - 1] ?? "";
  if (debug) console.error(`  tabRow ${computed.tabRow}: ${JSON.stringify(tabLine.slice(0, 30))}`);
  if (!tabLine.includes("BOARD")) failures.push(`tab row ${computed.tabRow} is not the tab strip`);
  for (const zone of computed.players) {
    const line = frameLines[zone.top - 1] ?? "";
    if (!line.includes(zone.value)) {
      failures.push(
        `row ${zone.top} should hold ${zone.value} but holds ${JSON.stringify(line.trim().slice(0, 40))}`,
      );
    }
  }

  // The mouse. These arrive as escape sequences through the same handler as
  // the keys, so they are testable the same way.
  const clickAt = (col: number, row: number): string => `${ESC}[<0;${col};${row}M`;
  const move = (col: number, row: number): string => `${ESC}[<35;${col};${row}M`;
  const wheelDown = (col: number, row: number): string => `${ESC}[<65;${col};${row}M`;

  // Terminals coalesce mouse reports, and motion reporting sends one per cell
  // crossed, so moving the pointer onto a tab and clicking it arrives as one
  // read holding several reports, sometimes cut in half. Every report starts
  // with the escape byte and Escape on the board quits, so a read the parser
  // did not recognise used to close the app. This is that read.
  const tabZone = computed.tabs[1];
  if (tabZone) {
    stdin.push(move(tabZone.left, tabZone.top) + move(tabZone.left + 1, tabZone.top));
    await settle();
    if (exited) failures.push("two mouse reports in one read quit the app");

    stdin.push(
      move(tabZone.left + 2, tabZone.top) + `${ESC}[<0;${tabZone.left + 2};${tabZone.top}`,
    );
    await settle();
    stdin.push("M");
    await settle();
    if (exited) failures.push("a mouse report split across two reads quit the app");
    if (where(latest()) !== tabZone.value) {
      failures.push(`a split click gave "${where(latest())}", expected "${tabZone.value}"`);
    }
    stdin.push("1");
    await settle();
  }

  // A lone escape byte on the board must not close the app. The terminal
  // reports the mouse as escape sequences, and a read that ends just after the
  // escape byte delivers it by itself, which the key parser cannot tell from
  // the Escape key. Quitting is q.
  stdin.push(ESC);
  await settle();
  stdin.push(ESC);
  await settle();
  if (exited) failures.push("escape on the board quit the app");

  const firstRow = computed.players[0];
  const thirdRow = computed.players[2];
  if (!firstRow || !thirdRow) {
    failures.push("layout produced no player rows to click");
  } else {
    stdin.push(clickAt(20, thirdRow.top));
    await settle();
    const clicked = latest();
    const markerLine = clicked.split(String.fromCharCode(10))[thirdRow.top - 1] ?? "";
    if (debug) console.error(`  click -> ${JSON.stringify(markerLine.trim().slice(0, 26))}`);
    if (!markerLine.includes(thirdRow.value)) {
      failures.push("clicking a row did not land on that row");
    }
    // The selection block sits just inside the panel border, not at column 0.
    if (!markerLine.slice(0, 6).includes(String.fromCharCode(0x2588))) {
      failures.push("clicking a row did not select it");
    }

    // Hovering a different row has to change the screen without selecting.
    const beforeHover = latest();
    stdin.push(move(20, firstRow.top));
    await settle();
    if (latest() === beforeHover) failures.push("hovering a row changed nothing");
  }

  const careerTab = computed.tabs[1];
  if (careerTab) {
    stdin.push(clickAt(careerTab.left + 1, careerTab.top));
    await settle();
    if (debug) console.error(`  tab click -> ${where(latest())}`);
    if (where(latest()) !== "career") failures.push("clicking the CAREER tab did not open it");
    stdin.push("1");
    await settle();
  }

  // The wheel scrolls a list, but only one too long to fit. In a tall window
  // the list fits and correctly does not move, so shrink first.
  stdout.resize(150, 22);
  await settle();
  stdin.push("5");
  await settle();
  const beforeWheel = latest();
  stdin.push(wheelDown(20, 10));
  await settle();
  stdin.push(wheelDown(20, 10));
  await settle();
  if (latest() === beforeWheel) failures.push("the wheel did not scroll the list");
  stdin.push("1");
  await settle();
  stdout.resize(150, 40);
  await settle();

  // Nothing may be taller than the window, or the terminal scrolls and the
  // user gets a scrollbar on a full-screen app.
  const tall = latest().split(String.fromCharCode(10)).length;
  if (debug) console.error(`  height: ${tall} lines in a ${stdout.rows}-row window`);
  if (tall > stdout.rows) failures.push(`frame is ${tall} lines in a ${stdout.rows}-row window`);

  // Resizing has to reflow, not just redraw at the old size.
  const wideFrame = latest();
  const wideMax = Math.max(...wideFrame.split(String.fromCharCode(10)).map((l) => l.length));
  stdout.resize(100, 40);
  await settle();
  await settle();
  const narrowFrame = latest();
  const narrowMax = Math.max(...narrowFrame.split(String.fromCharCode(10)).map((l) => l.length));
  if (debug) console.error(`  resize: ${wideMax} cols -> ${narrowMax} cols`);
  if (narrowMax >= wideMax) failures.push(`resize did not reflow (${wideMax} -> ${narrowMax})`);
  // A short window must lose the bottom of the table, never the key hints:
  // they are the only thing telling anyone what the app can do.
  stdout.resize(150, 18);
  await settle();
  await settle();
  const shortFrame = latest();
  const shortLines = shortFrame.split(String.fromCharCode(10)).length;
  if (debug)
    console.error(
      `  short window: ${shortLines} lines, footer ${shortFrame.includes("Quit") ? "kept" : "lost"}`,
    );
  if (shortLines > 18) failures.push(`frame is ${shortLines} lines in an 18-row window`);
  if (!shortFrame.includes("Quit")) failures.push("a short window pushed the key hints off screen");

  // Every view has to fit the window it is given. Scrolling a view that could
  // simply have drawn less is the failure this is here to prevent.
  for (const rows of [24, 32, 40]) {
    stdout.resize(150, rows);
    await settle();
    for (const [digit, name] of [
      ["1", "board"],
      ["2", "career"],
      ["3", "session"],
      ["4", "recap"],
      ["5", "encounters"],
    ] as Array<[string, string]>) {
      stdin.push(digit);
      await settle();
      const lines = latest().split(String.fromCharCode(10)).length;
      if (lines > rows) {
        failures.push(`${name} is ${lines} lines in a ${rows}-row window`);
      }
      if (!latest().includes("Quit")) {
        failures.push(`${name} lost the key hints at ${rows} rows`);
      }
    }
    // Every section of the detail panel, at every height. The panel holds the
    // densest data in the app and the window is the one thing it cannot
    // negotiate with, so a four figure number or a long name must shorten
    // rather than push the footer off the bottom.
    stdin.push("1");
    await settle();
    for (const section of ["stats", "form", "arsenal", "seen"]) {
      stdin.push("e");
      await settle();
      const lines = latest().split(String.fromCharCode(10)).length;
      if (lines > rows) {
        failures.push(`panel ${section} is ${lines} lines in a ${rows}-row window`);
      }
      if (!latest().includes("Quit")) {
        failures.push(`panel ${section} lost the key hints at ${rows} rows`);
      }
    }
    if (debug) console.error(`  fits at ${rows} rows`);
  }
  stdin.push("1");
  await settle();

  stdout.resize(150, 40);
  await settle();

  // Scrolling: a long list moves and comes back.
  stdout.resize(150, 22);
  await settle();
  stdin.push("5");
  await settle();
  const topOfList = latest();
  stdin.push("j");
  await settle();
  stdin.push("j");
  await settle();
  const scrolled = latest();
  if (debug) console.error(`  scroll -> ${topOfList === scrolled ? "stuck" : "moved"}`);
  if (topOfList === scrolled) failures.push("the encounter list did not scroll");
  stdin.push("g");
  await settle();
  if (latest() !== topOfList) failures.push("g did not return to the top of the list");
  stdin.push("1");
  await settle();

  // Filtering: typing narrows the board and escape puts it back.
  const unfiltered = latest();
  stdin.push("/");
  await settle();
  for (const ch of "neon") {
    stdin.push(ch);
    await settle();
  }
  const filtered = latest();
  if (debug) console.error(`  filter -> ${filtered.includes("NeonLock") ? "found" : "missing"}`);
  if (!filtered.includes("NeonLock")) failures.push("filtering lost the matching player");
  if (filtered.includes("SilentEnt")) failures.push("filtering kept a player that does not match");
  stdin.push(ESC);
  await settle();
  stdin.push(ESC);
  await settle();
  if (!latest().includes("SilentEnt")) failures.push("escape did not clear the filter");
  if (unfiltered.length === 0) failures.push("no baseline frame");

  // Idle repaint rate. Anything above a couple of frames a second is the app
  // fighting the console for no reason.
  const mark = stdout.frames.length;
  await new Promise((r) => setTimeout(r, 1000));
  const perSecond = stdout.frames.length - mark;
  if (debug) console.error(`  repaint: ${perSecond} frames/s while idle`);
  if (perSecond > 12) failures.push(`repainting ${perSecond} times a second while idle`);

  app.unmount();

  // The holding screen is what anyone waiting on the game actually looks at,
  // and it is the only screen with animation on it. Measure that separately.
  const idleOut = new FakeStdout(120);
  const idleIn = new TtyStdin();
  const idle = render(<App root={process.cwd()} preview={{ state: "OFFLINE", players: [] }} />, {
    stdout: idleOut as never,
    stdin: idleIn as never,
    exitOnCtrlC: false,
    patchConsole: false,
  });
  await settle();
  const holdMark = idleOut.frames.length;
  await new Promise((r) => setTimeout(r, 1000));
  const holdRate = idleOut.frames.length - holdMark;
  idle.unmount();
  if (debug) console.error(`  repaint: ${holdRate} frames/s on the holding screen`);
  if (holdRate > 12) failures.push(`holding screen repaints ${holdRate} times a second`);

  if (failures.length) {
    for (const f of failures) console.error(`  x ${f}`);
    process.exit(1);
  }
  console.log(`tui key-check OK (raw mode on, every view switches, ${perSecond} repaints/s idle)`);
  process.exit(0);
}

void main();
