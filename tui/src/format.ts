import type { Player, SessionPoint } from "./types.js";

// Pure formatting. Riot has shipped "" and null where a number belongs, and a
// missing field must cost one dash, never a crash -- the Python renderer this
// replaces had 22 separate ways to die on an absent key.

export function num(value: unknown): number | null {
  if (value === null || value === undefined || value === "") return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

export function trim(n: number): string {
  return Number.isInteger(n) ? String(n) : String(Number(n.toFixed(2)));
}

/** Shown wherever Riot gave us nothing. A hyphen, not a dash character. */
export const NONE = "-";

/** A percentage to one decimal, always. `-` when there is nothing to show. */
export function pct1(value: unknown): string {
  const n = num(value);
  return n === null ? NONE : `${n.toFixed(1)}%`;
}

export function dash(value: unknown, suffix = ""): string {
  const n = num(value);
  return n === null ? NONE : `${trim(n)}${suffix}`;
}

export function pad(
  text: string | number | null | undefined,
  width: number,
  align: "left" | "right" = "left",
): string {
  // Riot's JSON does not honour the types this code declares for it. A match
  // score arrives as a number, and `pad(m.score ?? "", 8)` then had a number to
  // pad: `.length` is undefined, so the clip never fired, and `.padEnd` does
  // not exist on a number. Opening a career threw and took the app with it.
  //
  // Coercing here is the whole fix. A guard at each call site is the same fix
  // written thirty times, and forgotten on the thirty first.
  const raw = text === null || text === undefined ? "" : String(text);
  // Code points rather than UTF-16 units, so a name with an emoji or an astral
  // character in it is never cut in half and left as a lone surrogate.
  const chars = Array.from(raw);
  const clipped =
    chars.length > width ? `${chars.slice(0, Math.max(0, width - 1)).join("")}.` : raw;
  return align === "right" ? clipped.padStart(width) : clipped.padEnd(width);
}

/** Always two decimals. The board and the match views disagreed otherwise:
 * one showed 3.00 and the other showed 3 for the same player. */
export function kd2(value: unknown): string {
  const n = num(value);
  return n === null ? NONE : n.toFixed(2);
}

export function isRanked(p: Player): boolean {
  return (num(p.rankTier) ?? 0) > 2;
}

/** Peak a full rank group or more above current: worth flagging. */
export function peakGap(p: Player): boolean {
  return (num(p.peakRankTier) ?? 0) - (num(p.rankTier) ?? 0) >= 3;
}

export function bar(value: number | null, max: number, cells: number): string {
  if (value === null) return "░".repeat(cells);
  const filled = Math.max(0, Math.min(cells, Math.round((value / max) * cells)));
  return "▇".repeat(filled) + "░".repeat(cells - filled);
}

export function meter(value: number | null, max: number, cells: number): string {
  if (value === null) return "▱".repeat(cells);
  const filled = Math.max(0, Math.min(cells, Math.round((value / max) * cells)));
  return "▰".repeat(filled) + "▱".repeat(cells - filled);
}

/** Riot has sent "" where a list belongs; "".slice() is a string, not a list. */
export function arr<T>(value: T[] | undefined | null): T[] {
  return Array.isArray(value) ? value : [];
}

export interface Pip {
  /** Stable among siblings: the same player's third result stays the third. */
  key: string;
  result: string;
}

export function formPips(p: Player): Pip[] {
  return arr(p.form)
    .slice(-5)
    .map((result, i) => ({ key: `${i}:${result}`, result }));
}

/** Total prior matches with this account, either side of the net. */
export function seenCount(p: Player): number {
  const e = p.encounter;
  if (!e) return 0;
  return (num(e.withCount) ?? 0) + (num(e.againstCount) ?? 0);
}

export function streakText(p: Player): string {
  const count = num(p.streak?.count) ?? 0;
  return count >= 3 ? `${p.streak?.type ?? ""}${count}` : "";
}

export function agoText(stamp: number | null, now: number): string {
  if (stamp === null) return "";
  const secs = Math.max(0, Math.round((now - stamp) / 1000));
  if (secs < 2) return "just now";
  if (secs < 60) return `${secs}s ago`;
  return `${Math.floor(secs / 60)}m ago`;
}

export interface FlowBar {
  key: string;
  delta: number;
  level: number;
  /** W, L or D. It was a bool, so a drawn match counted as a loss. */
  result: "W" | "L" | "D";
  map: string;
}

/** Session RR deltas bucketed to one of eight block heights, newest last. */
/** Riot's word for how a match ended, as the one letter that is drawn. */
export function outcomeOf(result: unknown): "W" | "L" | "D" {
  const word = String(result ?? "").toLowerCase();
  if (word === "victory") return "W";
  if (word === "draw") return "D";
  return "L";
}

export function rrFlow(points: SessionPoint[] | undefined): FlowBar[] {
  const list = arr(points);
  const deltas = list.map((p) => num(p.delta) ?? 0);
  const peak = Math.max(1, ...deltas.map((d) => Math.abs(d)));
  return list.map((p, i) => {
    const delta = deltas[i] ?? 0;
    return {
      key: p.matchId ?? `${i}:${p.map ?? ""}:${delta}`,
      delta,
      level: Math.max(1, Math.round((Math.abs(delta) / peak) * 8)),
      result: outcomeOf(p.result),
      map: p.map ?? "",
    };
  });
}

const BLOCKS = "▁▂▃▄▅▆▇█";

export function blockFor(level: number): string {
  return BLOCKS[Math.max(0, Math.min(BLOCKS.length - 1, level - 1))] ?? "▁";
}

export type SortMode = "party" | "rank" | "kd" | "win" | "level";

export const SORT_MODES: SortMode[] = ["party", "rank", "kd", "win", "level"];

export const SORT_LABEL: Record<SortMode, string> = {
  party: "party",
  rank: "rank",
  kd: "K/D",
  win: "win rate",
  level: "level",
};

/**
 * Default keeps party members adjacent so the rail can bracket them, strongest
 * first. The other modes are a flat ranking, because once you are sorting by
 * K/D the stack matters less than the number you sorted on.
 */
export function arrange(players: Player[], mode: SortMode = "party"): Player[] {
  const list = [...arr(players)];
  if (mode === "party") {
    // A guessed stack sorts after every confirmed party, but its members still
    // end up next to each other, which is the only way the bracket reads.
    const group = (p: Player): number => {
      const real = num(p.party?.number) ?? 0;
      if (real) return real;
      const id = p.stackGuess?.id;
      if (!id) return 0;
      return 100 + (Number.parseInt(id.replace(/\D+/g, ""), 10) || 0);
    };
    return list.sort((a, b) => {
      const ap = group(a);
      const bp = group(b);
      if (!!ap !== !!bp) return ap ? -1 : 1;
      if (ap !== bp) return ap - bp;
      return (num(b.rankTier) ?? 0) - (num(a.rankTier) ?? 0);
    });
  }
  const key = (p: Player): number => {
    if (mode === "rank") return num(p.rankTier) ?? -1;
    if (mode === "kd") return num(p.kd) ?? -1;
    if (mode === "win") return num(p.winRate) ?? -1;
    return num(p.level) ?? -1;
  };
  return list.sort((a, b) => key(b) - key(a));
}

/**
 * The bracket only means anything while party members are adjacent, which is
 * only true in party order. In any other sort it becomes a dot: still says
 * "these two are queued together", without drawing a bracket around players
 * who are not next to each other.
 */
export function railFor(players: Player[], index: number, grouped = true, stacks = true): string {
  // A party Riot confirmed outranks one the app guessed at, so a player who
  // has both is drawn as confirmed. A guess can be switched off entirely.
  const groupOf = (p: Player | undefined): string | undefined =>
    p?.party?.id ?? (stacks ? p?.stackGuess?.id : undefined) ?? undefined;
  if (!grouped) return groupOf(players[index]) ? (players[index]?.party ? "*" : "?") : " ";
  const id = groupOf(players[index]);
  if (!id) return " ";
  const prev = groupOf(players[index - 1]);
  const next = groupOf(players[index + 1]);
  if (id !== prev && id !== next) return " ";
  if (id !== prev) return "┌";
  if (id !== next) return "└";
  return "│";
}
