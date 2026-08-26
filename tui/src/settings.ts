import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

// Preferences live next to the other per-launch state in .overseer/. Nothing
// here reaches the bridge or Riot, so no setting in this file can change how
// many requests get made.

export interface Settings {
  detail: boolean;
  session: boolean;
  enemies: boolean;
  // One per board column. A column switched off here is not merely hidden:
  // it never enters the width budget, so the columns you did keep get the
  // room back rather than being shed on a narrow terminal.
  colAgent: boolean;
  colRank: boolean;
  colRr: boolean;
  colPeak: boolean;
  colKd: boolean;
  colWr: boolean;
  colGames: boolean;
  colHs: boolean;
  colMap: boolean;
  colSeen: boolean;
  colLvl: boolean;
  colForm: boolean;
  // The parts that are judgements rather than measurements.
  smurf: boolean;
  stacks: boolean;
  // The round-derived columns in the last match view.
  richRecap: boolean;
}

export const DEFAULTS: Settings = {
  detail: true,
  session: true,
  enemies: true,
  colAgent: true,
  colRank: true,
  colRr: true,
  colPeak: true,
  colKd: true,
  colWr: true,
  colGames: true,
  colHs: true,
  colMap: true,
  colSeen: true,
  colLvl: true,
  colForm: true,
  smurf: true,
  stacks: true,
  richRecap: true,
};

/** Which board column each setting governs, for the width budget. */
export const COLUMN_SETTING: Record<string, keyof Settings> = {
  agent: "colAgent",
  rank: "colRank",
  rr: "colRr",
  peak: "colPeak",
  kd: "colKd",
  wr: "colWr",
  games: "colGames",
  hs: "colHs",
  map: "colMap",
  seen: "colSeen",
  lvl: "colLvl",
  form: "colForm",
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
  /** Heading this option sits under. Repeats are drawn once. */
  group: string;
}

export const OPTIONS: Option[] = [
  { group: "Panels", key: "detail", label: "Detail panel", hint: "The selected player, in full" },
  { group: "Panels", key: "session", label: "Session panel", hint: "RR gained and lost today" },
  { group: "Panels", key: "enemies", label: "Enemy team", hint: "The other side, once in game" },
  { group: "Columns", key: "colAgent", label: "Agent", hint: "Who they are playing" },
  { group: "Columns", key: "colRank", label: "Rank", hint: "Current rank" },
  { group: "Columns", key: "colRr", label: "RR", hint: "Rank rating and the last change" },
  { group: "Columns", key: "colPeak", label: "Peak", hint: "Highest rank reached" },
  { group: "Columns", key: "colKd", label: "K/D", hint: "Kills over deaths" },
  { group: "Columns", key: "colWr", label: "Win rate", hint: "Share of matches won" },
  { group: "Columns", key: "colGames", label: "Games", hint: "Matches behind the numbers" },
  { group: "Columns", key: "colHs", label: "Headshot rate", hint: "Share of shots on the head" },
  { group: "Columns", key: "colMap", label: "Map record", hint: "How they do on this map" },
  { group: "Columns", key: "colSeen", label: "Met before", hint: "Times you have met them" },
  { group: "Columns", key: "colLvl", label: "Account level", hint: "The smurf tell" },
  { group: "Columns", key: "colForm", label: "Last five", hint: "Recent wins and losses" },
  { group: "Judgements", key: "smurf", label: "Smurf flags", hint: "A guess, from level and peak" },
  {
    group: "Judgements",
    key: "stacks",
    label: "Stack guesses",
    hint: "A guess, from who shares a side",
  },
  {
    group: "Last match",
    key: "richRecap",
    label: "Round detail",
    hint: "Econ, duels, clutches, spike",
  },
];
