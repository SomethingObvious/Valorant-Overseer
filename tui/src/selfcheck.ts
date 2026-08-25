import {
  bodyWidthOf,
  COLUMN_KEYS,
  cell,
  columnWidths,
  MIN_WIDTH,
  ROW_CHROME,
  visibleColumns,
} from "./app.js";
import { arrange, bar, blockFor, dash, meter, num, pad, railFor, rrFlow } from "./format.js";
import type { Player } from "./types.js";

// The Python renderer this replaces died 22 different ways on fields Riot
// drops -- hidden accounts, players with no competitive history, partial
// responses mid-patch. Every one of those shapes is exercised here.

const SAMPLE: Player = {
  puuid: "p1",
  name: "SilentEnt#GG",
  nameHidden: false,
  team: "Blue",
  isSelf: false,
  title: "Legend",
  agent: "KAY/O",
  agentColor: "#85929C",
  role: "Initiator",
  selection: "locked",
  rankTier: 20,
  rank: "Diamond 3",
  rankColor: "#D864C7",
  rr: 10,
  rrEarned: -18,
  leaderboard: 0,
  peakRankTier: 22,
  peakRank: "Ascendant 2",
  peakColor: "#189452",
  peakAct: "V25 Act 3",
  previousRank: "Diamond 3",
  winRate: 64,
  games: 349,
  kd: 1.9,
  hsPct: 13,
  skin: { name: "Transition" },
  weapons: [{ weapon: "Vandal", skin: { name: "Transition" } }],
  smurf: true,
  smurfReasons: ["Lvl 50, peak Ascendant 2"],
  streak: { type: "W", count: 3 },
  form: ["W", "W", "W", "L", "W"],
  topAgents: [{ agent: "KAY/O", games: 4 }],
  mapWinRate: { winRate: 67, games: 6 },
  level: 50,
  levelHidden: false,
  party: { id: "a", color: "#E34343", number: 1, size: 3 },
};

export function selfCheck(): string[] {
  const failures: string[] = [];

  const draw = (p: Player, label: string): void => {
    for (const key of COLUMN_KEYS) {
      try {
        cell(key, p, "┌", "#18E5A7");
      } catch (e) {
        failures.push(`${label}: column ${key} threw ${(e as Error).message}`);
      }
    }
    try {
      // The values the detail panel formats directly.
      dash(p.kd);
      dash(p.winRate, "%");
      dash(p.hsPct, "%");
      meter(num(p.rr), 100, 10);
      bar(num(p.kd), 2, 8);
      arrange([p, SAMPLE]);
      railFor([p, SAMPLE], 0);
    } catch (e) {
      failures.push(`${label}: detail formatting threw ${(e as Error).message}`);
    }
  };

  const fields = Object.keys(SAMPLE) as Array<keyof Player>;
  for (const field of fields) {
    const dropped: Player = { ...SAMPLE };
    delete dropped[field];
    draw(dropped, `missing ${String(field)}`);

    const nulled = { ...SAMPLE, [field]: null } as Player;
    draw(nulled, `null ${String(field)}`);

    const emptied = { ...SAMPLE, [field]: "" } as Player;
    draw(emptied, `empty ${String(field)}`);
  }
  draw({}, "wholly empty player");

  // A row must never be wider than the space it was given. When it is, Ink
  // wraps it and every player silently becomes two lines tall.
  for (let w = MIN_WIDTH; w < 400; w += 1) {
    // The number the renderer passes, not the terminal width. Checking the
    // latter validated a width the app never uses, so the two narrowest
    // terminals it accepted both wrapped and the check still passed.
    const body = bodyWidthOf(w);
    const keys = visibleColumns(body);
    const widths = columnWidths(keys, body);
    const used = keys.reduce((n, k) => n + (widths[k] ?? 0) + 1, ROW_CHROME);
    if (used > body) failures.push(`width ${w}: rows need ${used} columns and would wrap`);
  }

  // Columns may be shed on a narrow terminal, but never these, and the set
  // must never grow as the terminal shrinks.
  let seen = 0;
  for (let w = 20; w < 400; w += 2) {
    const cols = visibleColumns(w);
    for (const required of ["name", "rank", "kd"]) {
      if (!cols.includes(required)) failures.push(`width ${w}: lost the ${required} column`);
    }
    if (cols.length < seen) failures.push(`width ${w}: column count went backwards`);
    seen = cols.length;
  }

  // Session flow with nothing, one match, and lopsided swings.
  try {
    rrFlow(undefined);
    rrFlow([]);
    rrFlow([{ delta: 0, result: "Defeat" }]);
    rrFlow([
      { delta: 25, result: "Victory" },
      { delta: -3, result: "Defeat" },
    ]);
    for (let level = -2; level < 12; level += 1) blockFor(level);
    pad("a-very-long-player-name#TAG", 6);
    pad("", 4);
  } catch (e) {
    failures.push(`session formatting threw ${(e as Error).message}`);
  }

  return failures;
}
