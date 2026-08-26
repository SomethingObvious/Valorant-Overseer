import { useEffect, useState } from "react";
import type { Bridge } from "./bridge.js";
import type { Player } from "./types.js";

// Shapes the backend already serves over the bridge. Everything here is
// optional for the same reason the board fields are: these come from Riot's
// match history, which omits fields freely and returns partial data mid-patch.

export interface RankSummary {
  tier?: number | null | undefined;
  name?: string | undefined;
  group?: string | undefined;
  color?: string | undefined;
  rr?: number | null | undefined;
}

export interface PerfSummary {
  matches?: number | undefined;
  wins?: number | undefined;
  losses?: number | undefined;
  winRate?: number | null | undefined;
  net?: number | undefined;
  avgWin?: number | null | undefined;
  avgLoss?: number | null | undefined;
  current?: RankSummary | null | undefined;
  next?: RankSummary | null | undefined;
  exactResults?: number | undefined;
}

export interface SplitRow {
  name?: string | undefined;
  map?: string | undefined;
  agent?: string | undefined;
  games?: number | undefined;
  wins?: number | undefined;
  losses?: number | undefined;
  winRate?: number | null | undefined;
  kd?: number | null | undefined;
  net?: number | null | undefined;
}

export interface Insight {
  title?: string | undefined;
  detail?: string | undefined;
  text?: string | undefined;
  kind?: string | undefined;
  tone?: string | undefined;
}

export interface PerfPoint {
  ts?: number | undefined;
  rr?: number | undefined;
  tier?: number | undefined;
  delta?: number | undefined;
  map?: string | undefined;
  result?: string | undefined;
  matchId?: string | undefined;
}

export interface Performance {
  summary?: PerfSummary | undefined;
  points?: PerfPoint[] | undefined;
  insights?: Insight[] | undefined;
  splits?: {
    maps?: SplitRow[] | undefined;
    agents?: SplitRow[] | undefined;
    schedule?: SplitRow[] | undefined;
  };
  personalBests?: Record<string, unknown> | undefined;
  actComparison?: unknown | undefined;
  dataQuality?: unknown | undefined;
}

export interface TopWeapon {
  name?: string | undefined;
  kills?: number | undefined;
}

export interface RecapPlayer {
  puuid?: string | undefined;
  name?: string | undefined;
  team?: string | undefined;
  agent?: string | undefined;
  agentColor?: string | undefined;
  kills?: number | undefined;
  deaths?: number | undefined;
  assists?: number | undefined;
  kd?: number | null | undefined;
  acs?: number | null | undefined;
  hsPct?: number | null | undefined;
  rankTier?: number | undefined;
  rank?: string | undefined;
  rankColor?: string | undefined;
  /** Derived from roundResults, which match-details already returns. */
  adr?: number | null | undefined;
  kast?: number | null | undefined;
  econ?: number | null | undefined;
  firstBloods?: number | null | undefined;
  firstDeaths?: number | null | undefined;
  multiKills?: Record<string, number> | undefined;
  topWeapon?: TopWeapon | null | undefined;
  /** The rest of what the round data yields, all of it free. */
  weaponKills?: TopWeapon[] | undefined;
  clutches?: number | null | undefined;
  clutchesLost?: number | null | undefined;
  plants?: number | null | undefined;
  defuses?: number | null | undefined;
  shots?: number | null | undefined;
}

export interface Recap {
  matchId?: string | undefined;
  map?: string | undefined;
  mode?: string | undefined;
  result?: string | undefined;
  scores?: Record<string, number> | undefined;
  mvp?: RecapPlayer | null | undefined;
  teamMvp?: RecapPlayer | null | undefined;
  players?: RecapPlayer[] | undefined;
  you?: RecapPlayer | null | undefined;
  rrDelta?: number | null | undefined;
  tierAfter?: number | null | undefined;
  yourAvgKd?: number | null | undefined;
}

export interface EncounterRow {
  puuid?: string | undefined;
  name?: string | undefined;
  seen?: number | undefined;
  games?: number | undefined;
  withYou?: number | undefined;
  againstYou?: number | undefined;
  wins?: number | undefined;
  losses?: number | undefined;
  draws?: number | undefined;
  // The six counters the backend keeps, alongside the totals above. Both
  // are sent; the totals exist because a view wants one number, not six.
  withCount?: number | undefined;
  againstCount?: number | undefined;
  winsWith?: number | undefined;
  lossesWith?: number | undefined;
  drawsWith?: number | undefined;
  winsAgainst?: number | undefined;
  lossesAgainst?: number | undefined;
  drawsAgainst?: number | undefined;
  lastSeen?: number | undefined;
  lastMap?: string | undefined;
  rank?: string | undefined;
  rankColor?: string | undefined;
  agent?: string | undefined;
}

export interface Encounters {
  players?: EncounterRow[] | undefined;
  accountCount?: number | undefined;
  scope?: string | undefined;
}

export interface CareerMatch {
  matchId?: string | undefined;
  map?: string | undefined;
  mode?: string | undefined;
  agent?: string | undefined;
  result?: string | undefined;
  kills?: number | undefined;
  deaths?: number | undefined;
  assists?: number | undefined;
  kd?: number | null | undefined;
  acs?: number | null | undefined;
  hsPct?: number | null | undefined;
  rr?: number | null | undefined;
  delta?: number | null | undefined;
  ts?: number | undefined;
  score?: string | undefined;
}

export interface Career {
  name?: string | undefined;
  rank?: string | undefined;
  rankColor?: string | undefined;
  peakRank?: string | undefined;
  matches?: CareerMatch[] | undefined;
  agents?: SplitRow[] | undefined;
  maps?: SplitRow[] | undefined;
  summary?: PerfSummary | undefined;
  encounter?: EncounterRow | null | undefined;
  weapons?: Player["weapons"] | undefined;
  error?: string | undefined;
}

export type Phase = "idle" | "loading" | "ready" | "error";

export interface Fetched<T> {
  phase: Phase;
  data: T | null;
  error: string;
}

/**
 * Fetches once per key. `key` being null means "not needed yet", which is how a
 * view that is not open avoids ever asking -- the point being that nothing here
 * runs on a timer, so opening no views costs no Riot requests.
 */
export function useRequest<T>(
  bridge: Bridge | null,
  connected: boolean,
  name: string,
  key: string | null,
  params: Record<string, unknown> = {},
): Fetched<T> {
  const [state, setState] = useState<Fetched<T>>({ phase: "idle", data: null, error: "" });
  const paramKey = JSON.stringify(params);

  useEffect(() => {
    if (!bridge || !connected || key === null) return;
    let live = true;
    setState({ phase: "loading", data: null, error: "" });
    bridge
      .request<T>(name, JSON.parse(paramKey) as Record<string, unknown>)
      .then((data) => {
        if (live) setState({ phase: "ready", data, error: "" });
      })
      .catch((e: Error) => {
        if (live) setState({ phase: "error", data: null, error: e.message });
      });
    return () => {
      live = false;
    };
  }, [bridge, connected, name, key, paramKey]);

  return state;
}
