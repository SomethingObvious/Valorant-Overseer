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
