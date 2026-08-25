import { appendFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { render } from "ink";
import { App } from "./app.js";
import { selfCheck } from "./selfcheck.js";

// dist/overseer.js -> tui/ -> the install root
const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(HERE, "..", "..");

function argValue(name: string): string | null {
  const at = process.argv.indexOf(name);
  return at >= 0 ? (process.argv[at + 1] ?? null) : null;
}

const root = argValue("--root") ?? DEFAULT_ROOT;

if (process.argv.includes("--self-check")) {
  const failures = selfCheck();
  if (failures.length) {
    for (const line of failures) console.error(`  x ${line}`);
    process.exit(1);
  }
  console.log("tui self-check OK (formatting survives every missing field, columns hold)");
  process.exit(0);
}

// Startup diagnostics. Whether the terminal will give us a keyboard is not
// something the app can recover from, so record it where it can be read after
// the fact rather than guessed at.
try {
  mkdirSync(path.join(root, ".overseer"), { recursive: true });
  appendFileSync(
    path.join(root, ".overseer", "tui.log"),
    `${new Date().toISOString()} start isTTY=${process.stdin.isTTY === true}` +
      ` stdoutTTY=${process.stdout.isTTY === true}` +
      ` setRawMode=${typeof process.stdin.setRawMode === "function"}` +
      ` term=${process.env.TERM ?? ""} wt=${process.env.WT_SESSION ? "yes" : "no"}
`,
    "utf8",
  );
} catch {
  // Diagnostics must never be the reason the scoreboard fails to start.
}

// A render fault killed the app without leaving a word behind: the terminal
// closed, tui.log held nothing but a start line, and there was no way to tell
// what had happened. Record it, put the terminal back, and say where to look.
function record(kind: string, error: unknown): void {
  const detail = error instanceof Error ? (error.stack ?? error.message) : String(error);
  try {
    appendFileSync(
      path.join(root, ".overseer", "tui.log"),
      `${new Date().toISOString()} ${kind} ${detail}
`,
      "utf8",
    );
  } catch {
    // Nothing left to try. The console below is the last word.
  }
  if (process.stdout.isTTY) {
    process.stdout.write("[?1049l[?1003l[?1006l[?25h");
  }
  console.error(`
Valorant Overseer stopped: ${detail}`);
  console.error(`Recorded in .overseer/tui.log`);
  process.exit(1);
}

process.on("uncaughtException", (error) => record("crash", error));
process.on("unhandledRejection", (error) => record("crash-async", error));

process.title = "Valorant Overseer";
if (process.stdout.isTTY) {
  process.stdout.write("]0;Valorant Overseer");
}

const app = render(<App root={root} />);
await app.waitUntilExit();
