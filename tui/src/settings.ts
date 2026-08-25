import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

// Preferences live next to the other per-launch state in .overseer/. Nothing
// here reaches the bridge or Riot, so no setting in this file can change how
// many requests get made.

export interface Settings {
  detail: boolean;
  session: boolean;
  enemies: boolean;
}

export const DEFAULTS: Settings = {
  detail: true,
  session: true,
  enemies: true,
};

function file(root: string): string {
  return path.join(root, ".overseer", "tui.json");
}

export function load(root: string): Settings {
  try {
    const raw = readFileSync(file(root), "utf8");
    const parsed = JSON.parse(raw) as Partial<Settings>;
    return { ...DEFAULTS, ...parsed };
  } catch {
    return { ...DEFAULTS };
  }
}

export function save(root: string, settings: Settings): void {
  try {
    mkdirSync(path.dirname(file(root)), { recursive: true });
    writeFileSync(file(root), `${JSON.stringify(settings, null, 2)}\n`, "utf8");
  } catch {
    // Preferences failing to persist is not worth interrupting anyone over.
  }
}

export interface Option {
  key: keyof Settings;
  label: string;
  hint: string;
}

export const OPTIONS: Option[] = [
  { key: "detail", label: "Detail panel", hint: "Career card for the selected player" },
  { key: "session", label: "Session panel", hint: "RR gained and lost this session" },
  { key: "enemies", label: "Enemy team", hint: "Show the other team once a match starts" },
];
