// The board shape the Python bridge broadcasts. Every field is optional on
// purpose: Riot drops fields on hidden accounts, on players with no
// competitive history, and mid-patch, so nothing here may be assumed present.

export interface Skin {
  name?: string | undefined;
  icon?: string | undefined;
}

export interface WeaponSkin {
  weapon?: string | undefined;
  skin?: Skin | null | undefined;
}

export interface Party {
  id?: string | undefined;
  color?: string | undefined;
  number?: number | undefined;
  size?: number | undefined;
  members?: string[] | undefined;
}

/**
 * A party the app worked out rather than one Riot handed over. Riot only
 * reveals a party for accounts whose presence is visible, so for strangers
 * this is inference from how often they have shared a side, and `shared` and
 * `same` are the evidence it rests on.
 */
export interface StackGuess {
  id?: string | undefined;
  size?: number | undefined;
  confidence?: number | undefined;
  members?: string[] | undefined;
  shared?: number | undefined;
  same?: number | undefined;
}

export interface Streak {
  type?: string | undefined;
  count?: number | undefined;
}

export interface TopAgent {
  agent?: string | undefined;
  games?: number | undefined;
}

export interface MapWinRate {
  winRate?: number | undefined;
  games?: number | undefined;
}

/**
 * How often you have met this account before. The backend attaches this to
 * every live board already, from its own local log; it costs no request.
 */
export interface PlayerEncounter {
  withCount?: number | undefined;
  againstCount?: number | undefined;
  winsWith?: number | undefined;
  lossesWith?: number | undefined;
  winsAgainst?: number | undefined;
  lossesAgainst?: number | undefined;
  drawsWith?: number | undefined;
  drawsAgainst?: number | undefined;
}

export interface Player {
  puuid?: string | undefined;
  name?: string | undefined;
  nameHidden?: boolean | undefined;
  team?: string | undefined;
  isSelf?: boolean | undefined;
  title?: string | undefined;
  playerCard?: string | undefined;
  agent?: string | null | undefined;
  agentId?: string | undefined;
  agentPortrait?: string | undefined;
  agentArt?: string | undefined;
  agentColor?: string | undefined;
  role?: string | undefined;
  selection?: string | null | undefined;
  rankTier?: number | undefined;
  rank?: string | undefined;
  rankColor?: string | undefined;
  rankGroup?: string | undefined;
  rankIcon?: string | undefined;
  rr?: number | null | undefined;
  rrEarned?: number | null | undefined;
  leaderboard?: number | null | undefined;
  peakRankTier?: number | undefined;
  peakRank?: string | undefined;
  peakColor?: string | undefined;
  peakIcon?: string | undefined;
  peakAct?: string | undefined;
  previousRank?: string | undefined;
  winRate?: number | null | undefined;
  games?: number | undefined;
  kd?: number | null | undefined;
  hsPct?: number | null | undefined;
  skin?: Skin | null | undefined;
  weapons?: WeaponSkin[] | undefined;
  smurf?: boolean | undefined;
  smurfReasons?: string[] | undefined;
  streak?: Streak | null | undefined;
  form?: string[] | undefined;
  topAgents?: TopAgent[] | undefined;
  mapWinRate?: MapWinRate | null | undefined;
  level?: number | undefined;
  levelHidden?: boolean | undefined;
  party?: Party | null | undefined;
  stackGuess?: StackGuess | null | undefined;
  encounter?: PlayerEncounter | null | undefined;
}

export interface TeamStats {
  avgRankTier?: number | undefined;
  avgRank?: string | undefined;
  rankColor?: string | undefined;
  rankIcon?: string | undefined;
  avgKd?: number | undefined;
  avgWinRate?: number | undefined;
  smurfCount?: number | undefined;
  size?: number | undefined;
}

export interface SessionPoint {
  matchId?: string | undefined;
  ts?: number | undefined;
  map?: string | undefined;
  result?: string | undefined;
  delta?: number | undefined;
  tier?: number | undefined;
  rr?: number | undefined;
}

export interface Session {
  startedAt?: number | undefined;
  net?: number | undefined;
  points?: SessionPoint[] | undefined;
}

export interface Notice {
  level?: string | undefined;
  action?: string | undefined;
  message?: string | undefined;
}

export interface Score {
  ally?: number | undefined;
  enemy?: number | undefined;
  round?: number | undefined;
}

export interface LockProgress {
  locked?: number | undefined;
  total?: number | undefined;
}

export interface Board {
  state?: string | undefined;
  stateLabel?: string | undefined;
  source?: string | undefined;
  sourceDetail?: string | undefined;
  map?: string | undefined;
  mapSplash?: string | undefined;
  mode?: string | undefined;
  side?: string | null | undefined;
  matchId?: string | undefined;
  selfTeam?: string | undefined;
  selfPuuid?: string | undefined;
  players?: Player[] | undefined;
  teams?: Record<string, Player[]> | undefined;
  teamStats?: Record<string, TeamStats> | undefined;
  parties?: Party[] | undefined;
  score?: Score | null | undefined;
  lockProgress?: LockProgress | null | undefined;
  winProb?: number | null | undefined;
  session?: Session | null | undefined;
  notice?: Notice | null | undefined;
  error?: string | undefined;
  appVersion?: string | undefined;
}

export type ConnectionState = "connecting" | "live" | "lost";
