import { num } from "./format.js";

export const C = {
  red: "#FF4655",
  ally: "#18E5A7",
  enemy: "#FF4655",
  ice: "#9ADEFF",
  bone: "#ECE8E1",
  gold: "#FFB454",
  text: "#D6DDE3",
  dim: "#7E8C92",
  faint: "#55636D",
  line: "#2A3947",
  loss: "#FF8088",
  ink: "#0B1119",
} as const;

// One colour per rank group, in tier order, matching what the game shows.
//
// The backend sends a colour with every rank, but not on every path: the
// encounter log, a career row and a rank read that came back partial all leave
// it out, and the fallback was the ordinary text colour. Two players on the
// same rank then drew in two different colours depending on which code path
// filled them in, which is the sort of thing you cannot unsee once noticed.
// The tier is always there, so the colour comes off the tier and the field the
// backend sends is ignored.
const RANK_COLORS = [
  "#4A4A4A", // Unranked
  "#5A5751", // Iron
  "#BB8F5A", // Bronze
  "#AEB2B2", // Silver
  "#C5BA3F", // Gold
  "#18A7B9", // Platinum
  "#D864C7", // Diamond
  "#189452", // Ascendant
  "#DD4444", // Immortal
  "#FFFDCD", // Radiant
] as const;

const RANK_GROUPS = [
  "unranked",
  "iron",
  "bronze",
  "silver",
  "gold",
  "platinum",
  "diamond",
  "ascendant",
  "immortal",
  "radiant",
] as const;

/**
 * The colour for a rank, by tier when there is one and by name when there is
 * not. Tier 0 to 2 is unranked, then three tiers per group.
 *
 * The name is not a second opinion, it is the only key the encounter log and a
 * career row have: neither carries a tier, and colouring those two from the
 * field the backend sends is exactly how one rank ended up in two colours.
 */
export function rankColor(tier: unknown, name?: unknown): string {
  const t = num(tier);
  if (t !== null && t >= 3) {
    return RANK_COLORS[Math.min(RANK_COLORS.length - 1, Math.floor(t / 3))] ?? C.text;
  }
  const word = String(name ?? "")
    .trim()
    .toLowerCase();
  if (t === null && word) {
    const at = RANK_GROUPS.findIndex((g) => g !== "unranked" && word.startsWith(g));
    if (at > 0) return RANK_COLORS[at] ?? C.text;
  }
  return C.faint;
}

export function kdColor(kd: unknown): string {
  const n = num(kd);
  if (n === null) return C.faint;
  if (n >= 1.3) return C.ally;
  if (n >= 1.0) return C.ice;
  if (n >= 0.8) return C.bone;
  return C.loss;
}

export const ROLE_GLYPH: Record<string, string> = {
  Duelist: "▲",
  Initiator: "◆",
  Controller: "●",
  Sentinel: "■",
};

export const ROLE_COLOR: Record<string, string> = {
  Duelist: "#FF8A8A",
  Initiator: "#FFC46B",
  Controller: "#A99BFF",
  Sentinel: "#6BE3B8",
};

export const STATE_LABEL: Record<string, string> = {
  INGAME: "IN GAME",
  PREGAME: "AGENT SELECT",
  MENUS: "IN LOBBY",
  OFFLINE: "WAITING",
};

export const STATE_COLOR: Record<string, string> = {
  INGAME: C.red,
  PREGAME: C.gold,
  MENUS: C.ally,
  OFFLINE: C.dim,
};
