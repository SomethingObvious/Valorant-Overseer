import { CAREER, ENCOUNTERS, PERFORMANCE, RECAP } from "./fixtures.js";
import { SAMPLE_BOARD } from "./sample.js";
import type { Settings } from "./settings.js";
import type { Board, Player } from "./types.js";
import type { ViewName } from "./views.js";

// Every screen the app can show, named, so each one can be rendered on demand
// and diffed. This is what "test the UI" means for a program whose output is a
// picture: `node preview.mjs <story>` draws one, `--all` draws the lot, and the
// e2e script renders all of them and fails on a crash or an empty frame.

export interface Story {
  name: string;
  summary: string;
  board: Board;
  width: number;
  settings?: Partial<Settings> | undefined;
  openSettings?: boolean | undefined;
  view?: ViewName | undefined;
  help?: boolean | undefined;
  filter?: string | undefined;
  sort?: "party" | "rank" | "kd" | "win" | "level" | undefined;
  api?: Record<string, unknown> | undefined;
}

function board(over: Partial<Board>): Board {
  return { ...SAMPLE_BOARD, ...over };
}

const ALLY = SAMPLE_BOARD.teams?.Blue ?? [];

/** Marks whoever the board calls you as a flagged account, for the panel. */
const flagSelf = (p: Player): Player =>
  p.isSelf
    ? {
        ...p,
        smurf: true,
        smurfReasons: ["Lvl 41, peak Immortal 1", "88% win over 17 games"],
        leaderboard: 412,
      }
    : p;

/** Fields Riot drops on hidden accounts and players with no comp history. */
const SPARSE: Player[] = ALLY.map((p, i) => {
  if (i === 0) return { name: "OnlyAName#000", team: "Blue" };
  if (i === 1) return { ...p, rank: undefined, rankTier: undefined, kd: null, winRate: null };
  if (i === 2) return { ...p, form: undefined, topAgents: undefined, peakRank: undefined };
  if (i === 3) return { ...p, name: undefined, agent: null, level: undefined };
  return { ...p, nameHidden: true, levelHidden: true, party: undefined };
});

export const STORIES: Story[] = [
  {
    name: "ingame",
    summary: "Live match, both teams, win probability and session flow",
    board: SAMPLE_BOARD,
    width: 150,
  },
  {
    name: "pregame",
    summary: "Agent select: your team only, lock progress, no score yet",
    board: board({
      state: "PREGAME",
      stateLabel: "Agent Select",
      score: null,
      winProb: null,
      side: "Attack",
      lockProgress: { locked: 2, total: 5 },
      teams: { Blue: ALLY },
      players: ALLY,
    }),
    width: 150,
  },
  {
    name: "lobby",
    summary: "In the menus with a duo queued up",
    board: board({
      state: "MENUS",
      stateLabel: "In Lobby",
      map: undefined,
      mode: undefined,
      side: null,
      score: null,
      winProb: null,
      lockProgress: null,
      teams: { Blue: ALLY.slice(0, 2) },
      players: ALLY.slice(0, 2),
    }),
    width: 150,
  },
  {
    name: "holding",
    summary: "Game closed: the holding screen, nothing invented",
    board: {
      state: "OFFLINE",
      stateLabel: "Waiting for VALORANT",
      source: "idle",
      players: [],
      teams: {},
      notice: { level: "info", message: "Open VALORANT - lobby, Agent Select or a match." },
      appVersion: "2.2.0",
    },
    width: 100,
  },
  {
    name: "restart",
    summary: "Client unreadable: the error the backend actually reports",
    board: {
      state: "OFFLINE",
      stateLabel: "Waiting for VALORANT",
      source: "local",
      players: [],
      teams: {},
      notice: {
        level: "warn",
        action: "restart_game",
        message: "Couldn't read VALORANT - please restart your game, then try again.",
      },
    },
    width: 100,
  },
  {
    name: "sparse",
    summary: "Hidden and unranked accounts: the fields Riot leaves out",
    board: board({
      teams: { Blue: SPARSE },
      players: SPARSE,
      state: "PREGAME",
      score: null,
      winProb: null,
      teamStats: {},
      session: null,
    }),
    width: 150,
  },
  {
    // Agent select on a terminal too narrow for the mode label. The side has
    // to survive that, because it is what the screen is for.
    name: "pregame-narrow",
    summary: "70 columns in agent select: the side keeps its place",
    board: board({
      state: "PREGAME",
      stateLabel: "Agent Select",
      score: null,
      winProb: null,
      side: "Attack",
      lockProgress: { locked: 2, total: 5 },
      teams: { Blue: ALLY },
      players: ALLY,
    }),
    width: 70,
  },
  {
    // Columns switched off give their width back rather than being hidden
    // while still squeezing the row, so the ones that are left get more room.
    name: "columns-off",
    summary: "Peak, games, headshots and last five switched off in settings",
    board: SAMPLE_BOARD,
    width: 150,
    settings: { colPeak: false, colGames: false, colHs: false, colForm: false, stacks: false },
  },
  {
    // The panel shows the selected player, which starts as you, so its flags
    // only draw when you are the flagged one. They used to sit below the form
    // and the map record, off the bottom of the panel on any ordinary window.
    name: "detail-flagged",
    summary: "Detail panel for a flagged account: smurf, leaderboard, stack",
    board: board({
      players: (SAMPLE_BOARD.players ?? []).map(flagSelf),
      teams: {
        Blue: (SAMPLE_BOARD.teams?.Blue ?? []).map(flagSelf),
        Red: SAMPLE_BOARD.teams?.Red ?? [],
      },
    }),
    width: 150,
  },
  {
    // Riot's JSON does not honour the types declared for it: a match score
    // comes back as a number. Opening a career threw on exactly that, in pad,
    // and took the app with it, so every field here is the wrong shape.
    name: "career-wrong-types",
    summary: "career where Riot sent numbers for text and text for numbers",
    board: SAMPLE_BOARD,
    width: 150,
    view: "career",
    api: {
      profile: {
        name: 1234,
        matches: [
          { map: 7, agent: 42, score: 13, kills: "9", deaths: 4, kd: "1.4", acs: 231 },
          { map: null, score: 0, kills: 1, deaths: 1, assists: 1, kd: 1, acs: null },
        ],
        agents: [{ agent: 5, games: "12", winRate: 50 }],
        maps: [{ map: 9, games: 3, winRate: "33" }],
      },
    },
  },
  {
    // The backend answers with an error whenever the client is not in a match,
    // which is most of the time. Every view has to draw that rather than fall
    // over on a payload whose fields are all missing.
    name: "career-error",
    summary: "career view when the backend has nothing to give",
    board: SAMPLE_BOARD,
    width: 150,
    view: "career",
    api: { profile: { error: "No data available. Open VALORANT and sign in." } },
  },
  {
    name: "career-empty",
    summary: "career view on an answer with no fields at all",
    board: SAMPLE_BOARD,
    width: 150,
    view: "career",
    api: { profile: {} },
  },
  {
    // The backend answers with an error whenever the client is not in a match,
    // which is most of the time. Every view has to draw that rather than fall
    // over on a payload whose fields are all missing.
    name: "session-error",
    summary: "session view when the backend has nothing to give",
    board: SAMPLE_BOARD,
    width: 150,
    view: "session",
    api: { performance: { error: "No data available. Open VALORANT and sign in." } },
  },
  {
    name: "session-empty",
    summary: "session view on an answer with no fields at all",
    board: SAMPLE_BOARD,
    width: 150,
    view: "session",
    api: { performance: {} },
  },
  {
    // The backend answers with an error whenever the client is not in a match,
    // which is most of the time. Every view has to draw that rather than fall
    // over on a payload whose fields are all missing.
    name: "recap-error",
    summary: "recap view when the backend has nothing to give",
    board: SAMPLE_BOARD,
    width: 150,
    view: "recap",
    api: { recap: { error: "No data available. Open VALORANT and sign in." } },
  },
  {
    name: "recap-empty",
    summary: "recap view on an answer with no fields at all",
    board: SAMPLE_BOARD,
    width: 150,
    view: "recap",
    api: { recap: {} },
  },
  {
    // The backend answers with an error whenever the client is not in a match,
    // which is most of the time. Every view has to draw that rather than fall
    // over on a payload whose fields are all missing.
    name: "encounters-error",
    summary: "encounters view when the backend has nothing to give",
    board: SAMPLE_BOARD,
    width: 150,
    view: "encounters",
    api: { encounters: { error: "No data available. Open VALORANT and sign in." } },
  },
  {
    name: "encounters-empty",
    summary: "encounters view on an answer with no fields at all",
    board: SAMPLE_BOARD,
    width: 150,
    view: "encounters",
    api: { encounters: {} },
  },
  {
    name: "narrow",
    summary: "86 columns: sidebar gone, low-priority columns shed",
    board: SAMPLE_BOARD,
    width: 86,
  },
  {
    name: "tiny",
    summary: "62 columns: name, rank and K/D survive, nothing else",
    board: SAMPLE_BOARD,
    width: 62,
  },
  {
    name: "wide",
    summary: "190 columns: every column plus the detail panel",
    board: SAMPLE_BOARD,
    width: 190,
  },
  {
    name: "settings",
    summary: "The settings screen",
    board: SAMPLE_BOARD,
    width: 120,
    openSettings: true,
  },
  {
    name: "no-panels",
    summary: "Detail and session panels both switched off",
    board: SAMPLE_BOARD,
    width: 150,
    settings: { detail: false, session: false },
  },
  {
    name: "solo",
    summary: "No parties, no smurfs, no streaks: the quiet case",
    board: board({
      teams: {
        Blue: ALLY.map((p) => ({
          ...p,
          party: undefined,
          smurf: false,
          smurfReasons: [],
          streak: null,
        })),
      },
      players: [],
      state: "PREGAME",
      score: null,
      winProb: null,
      lockProgress: null,
    }),
    width: 150,
  },
  {
    name: "filtered",
    summary: "Board narrowed to the players matching a typed name",
    board: SAMPLE_BOARD,
    width: 150,
    filter: "pixel",
  },
  {
    name: "help",
    summary: "Every key the app answers to",
    board: SAMPLE_BOARD,
    width: 120,
    help: true,
  },
  {
    name: "sorted",
    summary: "Board re-sorted by K/D, with the order labelled",
    board: SAMPLE_BOARD,
    width: 150,
    sort: "kd",
  },
  {
    name: "career",
    summary: "Drill-down: agent and map splits, recent matches, history together",
    board: SAMPLE_BOARD,
    width: 150,
    view: "career",
    api: { profile: CAREER },
  },
  {
    name: "session",
    summary: "Ranked record, RR per match, splits by map and agent, insights",
    board: SAMPLE_BOARD,
    width: 150,
    view: "session",
    api: { performance: PERFORMANCE },
  },
  {
    name: "recap",
    summary: "Last match: every round-derived stat, on a wide terminal",
    board: SAMPLE_BOARD,
    width: 190,
    view: "recap",
    api: { recap: RECAP },
  },
  {
    name: "encounters",
    summary: "Everyone you have run into before, and your record with them",
    board: SAMPLE_BOARD,
    width: 110,
    view: "encounters",
    api: { encounters: ENCOUNTERS },
  },
  {
    name: "career-empty",
    summary: "A view with nothing to show yet",
    board: SAMPLE_BOARD,
    width: 110,
    view: "session",
  },
];

export function findStory(name: string): Story | undefined {
  return STORIES.find((s) => s.name === name);
}
