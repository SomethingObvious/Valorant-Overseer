import { Box, type Key, Text, useApp, useInput, useStdin, useStdout, useWindowSize } from "ink";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Career, Encounters, Fetched, Performance, Recap } from "./api.js";
import { useRequest } from "./api.js";
import { Bridge } from "./bridge.js";
import { brailleBars, colourRuns } from "./chart.js";
import {
  agoText,
  arr,
  arrange,
  bar,
  blockFor,
  dash,
  formPips,
  isRanked,
  kd2,
  meter,
  NONE,
  num,
  pad,
  peakGap,
  railFor,
  rrFlow,
  SORT_LABEL,
  SORT_MODES,
  type SortMode,
  seenCount,
  streakText,
} from "./format.js";
import { boardLayout, headerHeight } from "./layout.js";
import { enableMouse, hitTest, parseMouseChunk } from "./mouse.js";
import { enterAltScreen } from "./screen.js";
import * as prefs from "./settings.js";
import { Shimmer } from "./shimmer.js";
import { C, kdColor, ROLE_COLOR, ROLE_GLYPH, STATE_COLOR, STATE_LABEL } from "./theme.js";
import type { Board, ConnectionState, Player, TeamStats } from "./types.js";
import {
  CareerView,
  EncountersView,
  RecapView,
  SessionView,
  Tabs,
  VIEWS,
  type ViewName,
} from "./views.js";

const SIDEBAR = 38;

interface Column {
  header: string;
  width: number;
  prio: number;
  align?: "left" | "right";
}

// prio 0 never drops; 3 goes first as the terminal narrows. Widths come from
// the longest real string in each column ("Ascendant 3" is 11) so a header
// always sits directly over its own data instead of drifting off it.
const COLUMNS: Record<string, Column> = {
  rail: { header: " ", width: 2, prio: 0 },
  agent: { header: "AGENT", width: 9, prio: 1 },
  name: { header: "PLAYER", width: 17, prio: 0 },
  rank: { header: "RANK", width: 12, prio: 0 },
  rr: { header: "RR", width: 7, prio: 2, align: "right" },
  peak: { header: "PEAK", width: 13, prio: 1 },
  kd: { header: "K/D", width: 5, prio: 0, align: "right" },
  wr: { header: "WIN", width: 5, prio: 1, align: "right" },
  games: { header: "GAMES", width: 6, prio: 3, align: "right" },
  hs: { header: "HS", width: 4, prio: 3, align: "right" },
  map: { header: "MAP", width: 9, prio: 2, align: "right" },
  seen: { header: "MET", width: 4, prio: 1, align: "right" },
  // Level is the smurf tell: a level 40 account sitting in Immortal is the
  // thing you want to notice, so it outlasts the map record and RR on a
  // narrow terminal rather than being the first number to go.
  lvl: { header: "LVL", width: 5, prio: 1, align: "right" },
  form: { header: "LAST 5", width: 7, prio: 1 },
};

export const COLUMN_KEYS = Object.keys(COLUMNS);

/** Exposed so the self-check can prove a row never outgrows its panel. */
export const COLUMN_WIDTHS: Record<string, number> = Object.fromEntries(
  Object.entries(COLUMNS).map(([k, c]) => [k, c.width]),
);

// 2 panel border + 1 panel padding + 2 selection prefix + 5 trailing flag
// column. Get this wrong and the rows overflow and Ink wraps every one of
// them to two lines, which is subtle enough to ship twice.
export const ROW_CHROME = 10;
const NAME_MIN = 10;

/**
 * Below this the four columns that never drop cannot fit at their smallest, so
 * there is no table to draw. Saying so beats drawing a wrapped one.
 */
// The terminal width, not the width the table gets: the panel keeps two
// columns for its own border, so a row is fitted to width - 2. Naming the
// smaller number here let 43 and 44 through, where every row wrapped.
export const MIN_WIDTH = 45;

/** A result's colour. A draw is neither side's, so it takes neither's. */
export const outcomeColor = (letter: string): string =>
  letter === "W" ? C.ally : letter === "D" ? C.gold : C.loss;
/** What a row is actually fitted to, once the panel has taken its border. */
export const bodyWidthOf = (width: number): number => Math.max(1, width - 2);

/**
 * The widths to actually draw at. Once there is nothing left to drop, the name
 * gives up its own space rather than letting the row overflow: a clipped name
 * is readable, a row wrapped onto two lines is not.
 */
export function columnWidths(keys: string[], width: number): Record<string, number> {
  const out: Record<string, number> = {};
  for (const key of keys) out[key] = COLUMNS[key]?.width ?? 0;
  const used = (): number => keys.reduce((n, k) => n + (out[k] ?? 0) + 1, ROW_CHROME);
  if (out["name"] !== undefined) {
    const over = used() - width;
    if (over > 0) {
      out["name"] = Math.max(NAME_MIN, (out["name"] ?? 0) - over);
    }
  }
  return out;
}
const ORDER = COLUMN_KEYS;

export function visibleColumns(width: number): string[] {
  // Drop one column at a time, lowest priority and rightmost first, so a
  // terminal loses LVL before it loses AGENT. prio 0 never goes.
  const keep = new Set(ORDER);
  // 2 panel border + 1 panel padding + 2 selection prefix + 5 trailing flag
  // column. Get this wrong and the rows overflow and Ink wraps every one of
  // them to two lines, which is subtle enough to ship twice.
  const cost = (): number =>
    [...keep].reduce((n, k) => n + (COLUMNS[k]?.width ?? 0) + 1, ROW_CHROME);
  const order = [...ORDER]
    .map((k, i) => ({ k, i, prio: COLUMNS[k]?.prio ?? 0 }))
    .sort((a, b) => b.prio - a.prio || b.i - a.i);
  for (const { k, prio } of order) {
    if (cost() <= width) break;
    if (prio > 0) keep.delete(k);
  }
  return ORDER.filter((k) => keep.has(k));
}

/** Case-insensitive substring match on the name, which is what people type. */
function matching(players: Player[], filter: string): Player[] {
  const needle = filter.trim().toLowerCase();
  if (!needle) return players;
  return players.filter((p) => (p.name ?? "").toLowerCase().includes(needle));
}

function ordered(board: Board | null, showEnemies: boolean, sort: SortMode): Player[] {
  if (!board) return [];
  const teams = board.teams ?? {};
  const selfTeam = board.selfTeam ?? "Blue";
  const other = Object.keys(teams).find((t) => t !== selfTeam);
  const rows = arrange(arr(teams[selfTeam]), sort);
  if (showEnemies && board.state === "INGAME" && other) {
    rows.push(...arrange(arr(teams[other]), sort));
  }
  return rows;
}

// --- header ----------------------------------------------------------------

function Header({
  board,
  conn,
  lastAt,
  now,
  width,
  filter,
  filtering,
}: {
  board: Board;
  conn: ConnectionState;
  lastAt: number | null;
  now: number;
  width: number;
  filter: string;
  filtering: boolean;
}) {
  const state = board.state ?? "OFFLINE";
  const score = board.score;
  const prob = num(board.winProb);
  const session = board.session;
  const flow = rrFlow(session?.points);
  const net = num(session?.net) ?? 0;

  // Shed the least useful thing first as the terminal narrows. Wrapping is
  // never an option: the header is one line by definition, and a wrapped
  // wordmark reads as a rendering fault.
  const winCells = width >= 110 ? 16 : width >= 80 ? 10 : 6;
  const roomy = width >= 96;
  const medium = width >= 76;

  return (
    <Box flexDirection="column" paddingX={1} width={width} flexShrink={0}>
      <Box width={Math.max(10, width - 2)} flexShrink={0} overflowX="hidden">
        {/* The name of the app is the one thing on this line you already know.
            Below the medium width it goes, so the state, the map and the side
            keep their room instead of being truncated mid word. */}
        {medium ? (
          <>
            <Wordmark />
            <Text color={C.line}>{"  │  "}</Text>
          </>
        ) : null}
        {/* Truncate rather than wrap. The header is one line by definition,
            and layout.ts counts rows on that basis: a second line here puts
            every tab and player row one below where a click looks for it. */}
        <Text wrap="truncate" bold color={STATE_COLOR[state] ?? C.dim}>
          {state === "INGAME" ? "● " : "◆ "}
          {STATE_LABEL[state] ?? state}
        </Text>
        {board.map ? (
          <Text wrap="truncate" bold color={C.bone}>
            {"   "}
            {board.map}
          </Text>
        ) : null}
        {board.mode && roomy ? (
          <Text wrap="truncate" color={C.dim}>{`  ${board.mode}`}</Text>
        ) : null}
        {/* In agent select the side is the reason you are looking at this
            screen, because it is what you pick an agent for, so it keeps its
            place on a narrow terminal where the mode and the map record go. */}
        {board.side && (medium || board.state === "PREGAME") ? (
          <Text
            wrap="truncate"
            bold={board.state === "PREGAME"}
            color={board.side === "Attack" ? C.loss : C.ally}
          >
            {`  ${board.side.toUpperCase()}`}
          </Text>
        ) : null}
        {score ? (
          <>
            <Text>{"   "}</Text>
            <Text bold color={C.ally}>
              {num(score.ally) ?? 0}
            </Text>
            <Text wrap="truncate" color={C.faint}>
              :
            </Text>
            <Text bold color={C.enemy}>
              {num(score.enemy) ?? 0}
            </Text>
            <Text wrap="truncate" color={C.faint}>{`  RD ${num(score.round) ?? "?"}`}</Text>
          </>
        ) : null}
        {board.lockProgress ? (
          <Text wrap="truncate" color={C.gold}>
            {`   ${num(board.lockProgress.locked) ?? 0}/${num(board.lockProgress.total) ?? 0} locked`}
          </Text>
        ) : null}
        {filter || filtering ? (
          <>
            <Text color={C.line}>{"   /"}</Text>
            <Text bold color={C.gold}>
              {filter}
            </Text>
            {filtering ? <Text color={C.gold}>{"_"}</Text> : null}
          </>
        ) : null}
        <Box flexGrow={1} />
        <Text
          wrap="truncate"
          color={conn === "live" ? C.ally : conn === "connecting" ? C.gold : C.red}
        >
          {conn === "live" ? `● ${agoText(lastAt, now) || "live"}` : `○ ${conn}`}
        </Text>
      </Box>

      {prob !== null || flow.length ? (
        <Box
          width={Math.max(10, width - 2)}
          flexShrink={0}
          overflowX="hidden"
          marginTop={1}
          marginBottom={1}
        >
          {prob !== null ? (
            <>
              <Text wrap="truncate" color={C.dim}>
                {"Win "}
              </Text>
              <Text wrap="truncate" color={C.ally}>
                {"█".repeat(Math.round((prob / 100) * winCells))}
              </Text>
              <Text wrap="truncate" color={C.enemy}>
                {"█".repeat(winCells - Math.round((prob / 100) * winCells))}
              </Text>
              <Text
                wrap="truncate"
                color={prob >= 50 ? C.ally : C.loss}
              >{` ${Math.round(prob)}%`}</Text>
            </>
          ) : null}
          {flow.length ? (
            <>
              <Text wrap="truncate" color={C.line}>
                {prob !== null ? "   │   " : ""}
              </Text>
              <Text wrap="truncate" color={C.dim}>
                {"Session "}
              </Text>
              <Text bold color={net >= 0 ? C.ally : C.loss}>
                {`${net > 0 ? "+" : ""}${net} RR `}
              </Text>
              {flow.map((f) => (
                <Text key={f.key} color={f.delta >= 0 ? C.ally : C.loss}>
                  {blockFor(f.level)}
                </Text>
              ))}
              <Text wrap="truncate" color={C.faint}>
                {`  ${record(flow)}`}
              </Text>
            </>
          ) : null}
        </Box>
      ) : null}
    </Box>
  );
}

// --- rows ------------------------------------------------------------------

export function cell(
  key: string,
  p: Player,
  rail: string,
  teamColor: string,
  selected = false,
  drawWidth?: number,
): React.ReactNode {
  const col = COLUMNS[key];
  const w = drawWidth ?? col?.width ?? 0;
  const align = col?.align ?? "left";

  switch (key) {
    case "rail": {
      const role = p.role ?? "";
      return (
        <Text wrap="truncate">
          <Text color={p.party?.color ?? (p.stackGuess ? C.gold : C.line)} dimColor={!p.party}>
            {p.party || p.stackGuess ? rail || "|" : " "}
          </Text>
          <Text color={ROLE_COLOR[role] ?? C.line}>{ROLE_GLYPH[role] ?? " "}</Text>
        </Text>
      );
    }
    case "agent":
      return (
        <Text wrap="truncate" color={p.agent ? C.ice : C.faint}>
          {pad(p.agent ?? NONE, w)}
        </Text>
      );
    case "name": {
      const self = p.isSelf === true;
      return (
        <Text
          wrap="truncate"
          bold={self || selected}
          italic={p.nameHidden === true}
          color={
            selected
              ? C.bone
              : p.nameHidden
                ? C.faint
                : self
                  ? C.bone
                  : (p.party?.color ?? teamColor)
          }
        >
          {pad(p.name ?? NONE, w)}
        </Text>
      );
    }
    case "rank": {
      const tier = num(p.rankTier) ?? 0;
      return (
        <Text wrap="truncate" bold={tier > 2} color={tier <= 2 ? C.faint : (p.rankColor ?? C.text)}>
          {pad(p.rank ?? NONE, w)}
        </Text>
      );
    }
    case "rr": {
      if (!isRanked(p))
        return (
          <Text wrap="truncate" color={C.faint}>
            {pad(NONE, w, align)}
          </Text>
        );
      const earned = num(p.rrEarned);
      return (
        <Text wrap="truncate">
          <Text color={C.dim}>{pad(String(num(p.rr) ?? 0), w - 4, "right")}</Text>
          <Text color={earned === null || earned === 0 ? C.line : earned > 0 ? C.ally : C.loss}>
            {pad(earned ? `${earned > 0 ? "+" : ""}${earned}` : "", 4, "right")}
          </Text>
        </Text>
      );
    }
    case "peak":
      return (
        <Text wrap="truncate" color={peakGap(p) ? C.gold : (p.peakColor ?? C.dim)}>
          {pad(`${p.peakRank ?? NONE}${peakGap(p) ? " ^" : ""}`, w)}
        </Text>
      );
    case "kd": {
      return (
        <Text wrap="truncate" bold color={kdColor(p.kd)}>
          {pad(kd2(p.kd), w, align)}
        </Text>
      );
    }
    case "wr": {
      const wr = num(p.winRate);
      return (
        <Text wrap="truncate" color={wr === null ? C.faint : wr >= 50 ? C.ally : C.loss}>
          {pad(wr === null ? NONE : `${Math.round(wr)}%`, w, align)}
        </Text>
      );
    }
    case "games":
      return (
        <Text wrap="truncate" color={C.faint}>
          {pad(String(num(p.games) ?? 0), w, align)}
        </Text>
      );
    case "map": {
      const mapWr = p.mapWinRate;
      const games = num(mapWr?.games) ?? 0;
      if (!games)
        return (
          <Text wrap="truncate" color={C.line}>
            {pad(NONE, w, align)}
          </Text>
        );
      const rate = num(mapWr?.winRate) ?? 0;
      return (
        <Text wrap="truncate" color={rate >= 50 ? C.ally : C.loss}>
          {pad(`${Math.round(rate)}% ${games}g`, w, align)}
        </Text>
      );
    }
    case "seen": {
      const met = seenCount(p);
      return (
        <Text wrap="truncate" bold={met > 0} color={met > 0 ? C.gold : C.line}>
          {pad(met ? `${met}x` : NONE, w, align)}
        </Text>
      );
    }
    case "hs":
      return (
        <Text wrap="truncate" color={C.faint}>
          {pad(p.hsPct === null || p.hsPct === undefined ? NONE : `${num(p.hsPct)}`, w, align)}
        </Text>
      );
    case "lvl":
      return (
        <Text wrap="truncate" color={C.faint}>
          {pad(String(num(p.level) ?? NONE), w, align)}
        </Text>
      );
    case "form": {
      const pips = formPips(p);
      if (!pips.length) return <Text wrap="truncate">{pad("", w)}</Text>;
      return (
        <Text wrap="truncate">
          {pips.map((pip) => (
            <Text key={pip.key} bold color={outcomeColor(pip.result)}>
              {pip.result}
            </Text>
          ))}
          <Text>{" ".repeat(Math.max(0, w - pips.length))}</Text>
        </Text>
      );
    }
    default:
      return null;
  }
}

function PlayerRow({
  p,
  rail,
  cols,
  widths,
  teamColor,
  selected,
  hovered,
}: {
  p: Player;
  rail: string;
  cols: string[];
  widths: Record<string, number>;
  teamColor: string;
  selected: boolean;
  hovered: boolean;
}) {
  const streak = streakText(p);
  return (
    <Box>
      <Text bold color={selected ? C.red : hovered ? C.dim : C.line}>
        {selected ? "\u2588 " : hovered ? "\u2590 " : "  "}
      </Text>
      {cols.map((key) => (
        <Box key={key} width={widths[key] ?? 0} marginRight={1} flexShrink={0}>
          {cell(key, p, rail, teamColor, selected, widths[key])}
        </Box>
      ))}
      <Box width={5} flexShrink={0}>
        <Text wrap="truncate">
          {p.smurf ? (
            <Text bold color={C.gold}>
              {"! "}
            </Text>
          ) : (
            <Text>{"  "}</Text>
          )}
          {streak ? (
            <Text bold color={outcomeColor(streak.slice(0, 1))}>
              {streak}
            </Text>
          ) : null}
        </Text>
      </Box>
    </Box>
  );
}

// A team with nothing in it is an empty box with a title, which reads as a
// rendering fault. While a filter is on, a team with no match simply goes.
function TeamBlock({
  label,
  color,
  players,
  stats,
  cols,
  selected,
  hovered,
  width,
  sort,
}: {
  label: string;
  color: string;
  players: Player[];
  stats: TeamStats | undefined;
  cols: string[];
  selected: string | null;
  hovered: string | null;
  width: number;
  sort: SortMode;
}) {
  const widths = columnWidths(cols, width);
  const title = (
    <Text>
      <Text bold color={color}>
        {` ${label} `}
      </Text>
      {sort !== "party" ? (
        <>
          <Text color={C.line}>{"  "}</Text>
          <Text color={C.gold}>{`by ${SORT_LABEL[sort]}`}</Text>
        </>
      ) : null}
      {stats ? (
        <>
          <Text color={C.line}>{"  "}</Text>
          <Text color={stats.rankColor ?? C.dim}>{`${stats.avgRank ?? NONE} avg`}</Text>
          <Text color={C.line}>{"   "}</Text>
          <Text color={kdColor(stats.avgKd)}>{`${stats.avgKd ?? NONE} K/D`}</Text>
          <Text color={C.line}>{"   "}</Text>
          <Text color={C.dim}>{`${Math.round(num(stats.avgWinRate) ?? 0)}% win`}</Text>
          {num(stats.smurfCount) ? (
            <>
              <Text color={C.line}>{"   "}</Text>
              <Text bold color={C.gold}>{`! ${num(stats.smurfCount)} flagged `}</Text>
            </>
          ) : (
            <Text> </Text>
          )}
        </>
      ) : null}
    </Text>
  );

  if (!players.length) return null;

  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      borderDimColor
      width={width}
      paddingLeft={1}
      marginBottom={1}
      flexShrink={0}
    >
      <Box marginTop={-1} marginLeft={1}>
        {title}
      </Box>
      <Box marginBottom={0}>
        <Text>{"  "}</Text>
        {cols.map((key) => (
          <Box key={key} width={widths[key] ?? 0} marginRight={1} flexShrink={0}>
            <Text bold wrap="truncate" color={C.faint}>
              {pad(COLUMNS[key]?.header ?? "", widths[key] ?? 0, COLUMNS[key]?.align)}
            </Text>
          </Box>
        ))}
      </Box>
      {players.map((p, i) => (
        <PlayerRow
          key={p.puuid ?? i}
          p={p}
          rail={railFor(players, i, sort === "party")}
          cols={cols}
          widths={widths}
          teamColor={color}
          selected={p.puuid === selected}
          hovered={p.puuid === hovered}
        />
      ))}
    </Box>
  );
}

// --- sidebar ---------------------------------------------------------------

// Two is a duo, five is the whole team. Said the way a player would say it.
const STACK_NAME: Record<number, string> = {
  2: "duo",
  3: "trio",
  4: "four stack",
  5: "five stack",
};

function Detail({ p }: { p: Player | null }) {
  if (!p) {
    return (
      <Box borderStyle="round" borderColor={C.line} paddingX={1} width={SIDEBAR}>
        <Text wrap="truncate" color={C.faint}>
          No player selected.
        </Text>
      </Box>
    );
  }
  const mapWr = p.mapWinRate;
  const reasons = arr(p.smurfReasons);
  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      borderDimColor
      paddingX={1}
      marginBottom={1}
      width={SIDEBAR}
    >
      <Text bold color={C.bone}>
        {p.name ?? NONE}
      </Text>
      <Text wrap="truncate" color={C.dim}>
        {p.title ? `${p.title} · ` : ""}
        {`Level ${num(p.level) ?? NONE}`}
        {p.role ? ` · ${p.role}` : ""}
      </Text>
      <Box marginTop={1}>
        <Text wrap="truncate" color={p.rankColor ?? C.text}>
          {p.rank ?? NONE}
        </Text>
        {isRanked(p) ? <Text wrap="truncate" color={C.dim}>{`  ${num(p.rr) ?? 0} RR`}</Text> : null}
      </Box>
      {isRanked(p) ? (
        <Text wrap="truncate" color={C.ice}>
          {meter(num(p.rr), 100, 10)}
        </Text>
      ) : null}
      <Text wrap="truncate" color={peakGap(p) ? C.gold : C.dim}>
        {`Peak ${p.peakRank ?? NONE}${p.peakAct ? ` · ${p.peakAct}` : ""}`}
      </Text>
      {p.previousRank ? (
        <Text wrap="truncate" color={C.faint}>{`Last act ${p.previousRank}`}</Text>
      ) : null}

      <Box marginTop={1}>
        <Text wrap="truncate" color={C.dim}>
          {"K/D   "}
        </Text>
        <Text wrap="truncate" color={kdColor(p.kd)}>{`${dash(p.kd)} ${bar(num(p.kd), 2, 8)}`}</Text>
      </Box>
      <Box>
        <Text wrap="truncate" color={C.dim}>
          {"Win   "}
        </Text>
        <Text wrap="truncate" color={(num(p.winRate) ?? 0) >= 50 ? C.ally : C.text}>
          {`${dash(p.winRate, "%")} over ${num(p.games) ?? 0}`}
        </Text>
      </Box>
      <Box>
        <Text wrap="truncate" color={C.dim}>
          {"HS    "}
        </Text>
        <Text wrap="truncate" color={C.text}>
          {dash(p.hsPct, "%")}
        </Text>
        {mapWr && (num(mapWr.games) ?? 0) > 0 ? (
          <Text
            wrap="truncate"
            color={C.dim}
          >{`   This map ${num(mapWr.winRate) ?? 0}% (${num(mapWr.games)})`}</Text>
        ) : null}
      </Box>

      {formPips(p).length ? (
        <Box marginTop={1}>
          <Text wrap="truncate" color={C.dim}>
            {"Form  "}
          </Text>
          {formPips(p).map((pip) => (
            <Text key={pip.key} color={outcomeColor(pip.result)}>
              {`${pip.result} `}
            </Text>
          ))}
          <Text wrap="truncate" color={C.faint}>
            {streakText(p)}
          </Text>
        </Box>
      ) : null}

      {arr(p.topAgents).length ? (
        <Box>
          <Text wrap="truncate" color={C.dim}>
            {"Mains "}
          </Text>
          <Text wrap="truncate" color={C.text}>
            {` ${arr(p.topAgents)
              .map((a) => `${a.agent ?? "?"} ${num(a.games) ?? 0}`)
              .join("  ")}`}
          </Text>
        </Box>
      ) : null}

      {arr(p.weapons).length ? (
        <Box>
          <Text wrap="truncate" color={C.dim}>
            {"Skins "}
          </Text>
          <Text wrap="truncate" color={C.ice}>
            {` ${arr(p.weapons)
              .slice(0, 2)
              .map((w) => w.skin?.name ?? NONE)
              .join("  ")}`}
          </Text>
        </Box>
      ) : null}

      {reasons.length ? (
        <Box flexDirection="column" marginTop={1}>
          <Text bold color={C.gold}>
            ⚑ Smurf
          </Text>
          {reasons.map((r) => (
            <Text key={r} color={C.gold}>
              {`  ${r}`}
            </Text>
          ))}
        </Box>
      ) : null}

      {p.stackGuess && !p.party ? (
        <Box flexDirection="column" marginTop={1}>
          <Text bold color={C.gold}>
            {`Probably a ${STACK_NAME[num(p.stackGuess.size) ?? 0] ?? "stack"}`}
          </Text>
          <Text wrap="truncate" color={C.faint}>
            {`  ${num(p.stackGuess.same) ?? 0}/${num(p.stackGuess.shared) ?? 0} same side` +
              `, ${num(p.stackGuess.confidence) ?? 0}% sure`}
          </Text>
        </Box>
      ) : null}

      {seenCount(p) ? (
        <Box flexDirection="column" marginTop={1}>
          <Text bold color={C.gold}>
            {`Met ${seenCount(p)} time${seenCount(p) === 1 ? "" : "s"} before`}
          </Text>
          {num(p.encounter?.withCount) ? (
            <Text color={C.faint}>
              {`  ${num(p.encounter?.withCount)} on your team` +
                `, ${num(p.encounter?.winsWith) ?? 0}W-${num(p.encounter?.lossesWith) ?? 0}L` +
                `${num(p.encounter?.drawsWith) ? `-${num(p.encounter?.drawsWith)}D` : ""}`}
            </Text>
          ) : null}
          {num(p.encounter?.againstCount) ? (
            <Text color={C.faint}>
              {`  ${num(p.encounter?.againstCount)} against you` +
                `, ${num(p.encounter?.winsAgainst) ?? 0}W-` +
                `${num(p.encounter?.lossesAgainst) ?? 0}L` +
                `${num(p.encounter?.drawsAgainst) ? `-${num(p.encounter?.drawsAgainst)}D` : ""}`}
            </Text>
          ) : null}
        </Box>
      ) : null}

      <Box marginTop={1}>
        <Text wrap="truncate" color={C.line}>
          {"[Enter]"}
        </Text>
        <Text wrap="truncate" color={C.faint}>
          {" Career and match history"}
        </Text>
      </Box>
    </Box>
  );
}

// Agent select only. Every value here comes off the board, so this costs no
// request: the roles are already on the players and the lock state is already
// on the board. A composition with no controller or no initiator is the thing
// people notice thirty seconds too late.
const ROLE_ORDER = ["Duelist", "Initiator", "Controller", "Sentinel"] as const;

function TeamComp({ players, board }: { players: Player[]; board: Board }) {
  const counts = new Map<string, number>();
  let unpicked = 0;
  for (const p of players) {
    const role = p.role ?? "";
    if (!role) {
      unpicked += 1;
      continue;
    }
    counts.set(role, (counts.get(role) ?? 0) + 1);
  }

  const missing = ROLE_ORDER.filter((role) => !counts.get(role));
  const locked = num(board.lockProgress?.locked) ?? 0;
  const total = num(board.lockProgress?.total) ?? players.length;

  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      borderDimColor
      paddingX={1}
      marginBottom={1}
      width={SIDEBAR}
    >
      <Box>
        <Text bold color={C.dim}>
          TEAM COMP
        </Text>
        <Text color={C.faint}>{`   ${locked}/${total} locked`}</Text>
      </Box>
      <Box height={1} />
      {ROLE_ORDER.map((role) => {
        const n = counts.get(role) ?? 0;
        return (
          <Box key={role}>
            <Text color={n ? (ROLE_COLOR[role] ?? C.text) : C.line}>
              {`${ROLE_GLYPH[role] ?? " "} `}
            </Text>
            <Box width={12} flexShrink={0}>
              <Text color={n ? C.text : C.faint}>{role}</Text>
            </Box>
            <Text bold color={n ? (ROLE_COLOR[role] ?? C.text) : C.line}>
              {n ? "#".repeat(Math.min(n, 5)) : "-"}
            </Text>
          </Box>
        );
      })}
      {unpicked ? (
        <Box marginTop={1}>
          <Text color={C.faint}>{`${unpicked} still picking`}</Text>
        </Box>
      ) : null}
      {missing.length ? (
        <Box marginTop={1} flexDirection="column">
          <Text bold color={C.gold}>
            {`No ${missing.map((m) => m.toLowerCase()).join(", no ")}`}
          </Text>
        </Box>
      ) : (
        <Box marginTop={1}>
          <Text color={C.ally}>All four roles covered</Text>
        </Box>
      )}
    </Box>
  );
}

// A session reads 3W-2L, and 3W-2L-1D when there was a draw. Drawn matches
// are rare enough that a D on every session would be noise, and common
// enough that folding them into the losses was a lie.
function record(flow: ReturnType<typeof rrFlow>): string {
  const count = (letter: "W" | "L" | "D"): number => flow.filter((f) => f.result === letter).length;
  const draws = count("D");
  return `${count("W")}W-${count("L")}L${draws ? `-${draws}D` : ""}`;
}

function Session({ board }: { board: Board }) {
  const flow = rrFlow(board.session?.points);
  if (!flow.length) return null;
  const net = num(board.session?.net) ?? 0;
  const wins = flow.filter((f) => f.result === "W").length;
  // Six matches across a 38-column panel is three braille cells and unreadable.
  // Widen each match into several sub-columns so the chart fills the panel;
  // 4 keeps a long session legible without turning a short one into slabs.
  const perMatch = Math.max(1, Math.min(4, Math.floor(((SIDEBAR - 4) * 2) / flow.length)));
  const columns = flow.flatMap((f) =>
    Array.from({ length: perMatch }, () => ({ value: f.level / 8, positive: f.delta >= 0 })),
  );
  const rows = brailleBars(columns, 3);
  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      paddingX={1}
      width={SIDEBAR}
    >
      <Box>
        <Text bold color={C.dim}>
          SESSION RR
        </Text>
        <Text bold color={net >= 0 ? C.ally : C.loss}>
          {`  ${net > 0 ? "+" : ""}${net} RR`}
        </Text>
        <Text color={C.faint}>{`  ${wins}W-${flow.length - wins}L`}</Text>
      </Box>
      {rows.map((row) => (
        <Box key={row.key}>
          {colourRuns(row).map((run) => (
            <Text key={run.key} color={run.positive ? C.ally : C.loss}>
              {run.text}
            </Text>
          ))}
        </Box>
      ))}
      <Text color={C.faint}>{"One bar per match. "}</Text>
      <Text>
        <Text color={C.ally}>{"Green"}</Text>
        <Text color={C.faint}>{" gained RR, "}</Text>
        <Text color={C.loss}>{"red"}</Text>
        <Text color={C.faint}>{" lost it."}</Text>
      </Text>
    </Box>
  );
}

// --- settings --------------------------------------------------------------

function SettingsView({ settings, cursor }: { settings: prefs.Settings; cursor: number }) {
  return (
    <Box flexDirection="column" borderStyle="round" borderColor={C.red} paddingX={2} paddingY={1}>
      <Text bold color={C.bone}>
        Settings
      </Text>
      <Text wrap="truncate" color={C.faint}>
        {"Stored in .overseer/tui.json."}
      </Text>
      <Box height={1} />
      {prefs.OPTIONS.map((opt, i) => {
        const active = i === cursor;
        const value = settings[opt.key];
        const shown = value ? "on" : "off";
        return (
          <Box key={opt.key}>
            <Text wrap="truncate" color={active ? C.red : C.faint}>
              {active ? "▸ " : "  "}
            </Text>
            <Text wrap="truncate" bold={active} color={active ? C.bone : C.text}>
              {pad(opt.label, 16)}
            </Text>
            <Text bold color={value ? C.ally : C.faint}>
              {pad(shown, 6)}
            </Text>
            <Text wrap="truncate" color={C.faint}>
              {opt.hint}
            </Text>
          </Box>
        );
      })}
      <Box height={1} />
      <Text wrap="truncate" color={C.faint}>
        {" "}
        <KeyHints
          pairs={[
            ["↑↓", "Move"],
            ["Enter", "Toggle"],
            [",", "Close"],
          ]}
        />
      </Text>
    </Box>
  );
}

// --- holding ---------------------------------------------------------------

// A row of dots with one lit, rather than a spinner glyph: it sits on its own
// line so every line of the holding screen centres on the same axis.
const DOTS = ["a", "b", "c", "d", "e"];

// One <Text>, not a <Box> of them. Ink rounds a centred Box the opposite way
// from a centred Text, so as a Box this sat one column right of every other
// line on the holding screen and the two words read as off-centre.
function Wordmark({ animate }: { animate?: boolean }): React.ReactElement {
  return (
    <Text>
      <Text bold color={C.red}>
        VALORANT
      </Text>
      <Text> </Text>
      {animate ? (
        <Shimmer text="OVERSEER" tones={WORDMARK_TONES} bold />
      ) : (
        <Text bold color={C.bone}>
          OVERSEER
        </Text>
      )}
    </Text>
  );
}

const WORDMARK_TONES = {
  base: C.dim,
  trail: C.text,
  core: C.bone,
  lead: C.text,
} as const;

function Holding({
  board,
  conn,
  detail,
  tick,
  animate,
}: {
  board: Board;
  conn: ConnectionState;
  detail: string;
  tick: number;
  animate: boolean;
}) {
  const message =
    board.notice?.message ??
    board.error ??
    "Open the game and join a lobby, Agent Select or a match.";
  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      paddingX={4}
      paddingY={2}
      alignItems="center"
    >
      <Wordmark animate={animate} />
      <Box height={1} />
      <Text bold color={C.bone}>
        {conn === "live" ? "Waiting for VALORANT" : "Starting up"}
      </Text>
      <Box height={1} />
      <Box>
        {DOTS.map((dot, i) => (
          <Text key={dot} color={animate && tick % DOTS.length === i ? C.gold : C.line}>
            {"● "}
          </Text>
        ))}
      </Box>
      <Box height={1} />
      <Text wrap="truncate" color={C.dim}>
        {message}
      </Text>
      <Box height={1} />
      <Text wrap="truncate" color={conn === "live" ? C.faint : C.gold}>
        {conn === "live"
          ? "Bridge connected. Local client not ready."
          : detail
            ? `${detail.charAt(0).toUpperCase()}${detail.slice(1)}.`
            : "Connecting to the backend."}
      </Text>
    </Box>
  );
}

// --- app -------------------------------------------------------------------

const HELP_SECTIONS: Array<[string, Array<[string, string]>]> = [
  [
    "Moving around",
    [
      ["up down", "Move the selection"],
      ["j k", "Move the selection"],
      ["g G", "First player, last player"],
      ["Enter", "Open the selected player's career"],
    ],
  ],
  [
    "Views",
    [
      ["1", "Board"],
      ["2", "Career of the selected player"],
      ["3", "Session"],
      ["4", "Last match"],
      ["5", "Everyone you have seen before"],
      ["Tab", "Next view"],
      ["Esc", "Back to the board"],
    ],
  ],
  [
    "The board",
    [
      ["s", "Sort: party, rank, K/D, win rate, level"],
      ["/", "Filter by name, Esc clears it"],
      ["r", "Ask the backend again for this view"],
      ["d", "Detail panel on and off"],
    ],
  ],
  [
    "The rail on the left",
    [
      ["colour", "A party Riot confirmed, from presence"],
      ["gold", "A stack the app guessed, from history"],
      ["select", "The guess and its evidence, in the panel"],
    ],
  ],
  [
    "Mouse",
    [
      ["click", "Select a player, or open a view"],
      ["hover", "Highlights what you are over"],
      ["wheel", "Scroll a list, or move the selection"],
    ],
  ],
  [
    "Everything else",
    [
      [",", "Settings"],
      ["?", "This list"],
      ["q", "Quit"],
    ],
  ],
];

function HelpView({ width }: { width: number }) {
  const columns = width >= 96 ? 2 : 1;
  const left = HELP_SECTIONS.filter((_, i) => i % columns === 0);
  const right = columns === 2 ? HELP_SECTIONS.filter((_, i) => i % 2 === 1) : [];

  const column = (sections: typeof HELP_SECTIONS) => (
    <Box flexDirection="column" width={columns === 2 ? Math.floor((width - 8) / 2) : undefined}>
      {sections.map(([title, rows]) => (
        <Box key={title} flexDirection="column" marginBottom={1}>
          <Text bold color={C.red}>
            {title}
          </Text>
          {rows.map(([keyName, what]) => (
            <Box key={keyName}>
              <Box width={11} flexShrink={0}>
                <Text color={C.line}>{"["}</Text>
                <Text bold color={C.dim}>
                  {keyName}
                </Text>
                <Text color={C.line}>{"]"}</Text>
              </Box>
              <Text color={C.text}>{what}</Text>
            </Box>
          ))}
        </Box>
      ))}
    </Box>
  );

  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      borderDimColor
      paddingX={2}
      paddingY={1}
    >
      <Box marginBottom={1}>
        <Text bold color={C.bone}>
          Keys
        </Text>
        <Text color={C.faint}>{"   Nothing here touches the game. Every key moves the view."}</Text>
      </Box>
      <Box>
        {column(left)}
        {right.length ? <Box marginLeft={2}>{column(right)}</Box> : null}
      </Box>
    </Box>
  );
}

function KeyHints({ pairs }: { pairs: Array<[string, string]> }) {
  return (
    <Text wrap="truncate">
      {pairs.map(([key, label], i) => (
        <Text key={key}>
          {i > 0 ? <Text color={C.line}>{"   "}</Text> : null}
          <Text color={C.line}>{"["}</Text>
          <Text bold color={C.dim}>
            {key}
          </Text>
          <Text color={C.line}>{"] "}</Text>
          <Text color={C.faint}>{label}</Text>
        </Text>
      ))}
    </Text>
  );
}

function Keys({ onKey }: { onKey: (input: string, key: Key) => void }) {
  useInput(onKey);
  return null;
}

export function App({
  root,
  preview,
  previewSettings,
  previewOpenSettings,
  previewHelp,
  previewFilter,
  previewSort,
  previewView,
  previewApi,
}: {
  root: string;
  preview?: Board | undefined;
  previewSettings?: Partial<prefs.Settings> | undefined;
  previewOpenSettings?: boolean | undefined;
  previewHelp?: boolean | undefined;
  previewFilter?: string | undefined;
  previewSort?: SortMode | undefined;
  previewView?: ViewName | undefined;
  /** Canned bridge responses, so a story can draw a populated view. */
  previewApi?: Record<string, unknown> | undefined;
}) {
  const { exit } = useApp();
  const [board, setBoard] = useState<Board | null>(preview ?? null);
  const [conn, setConn] = useState<ConnectionState>(preview ? "live" : "connecting");
  const [connDetail, setConnDetail] = useState("");
  const [lastAt, setLastAt] = useState<number | null>(null);
  const [settings, setSettings] = useState<prefs.Settings>(() =>
    preview ? { ...prefs.DEFAULTS, ...previewSettings } : prefs.load(root),
  );
  const [selected, setSelected] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(previewOpenSettings === true);
  const [view, setView] = useState<ViewName>(previewView ?? "board");
  const [sort, setSort] = useState<SortMode>(previewSort ?? "party");
  const [offset, setOffset] = useState(0);
  // Bumped by r. It rides along in the request key, so asking again is the
  // same code path as opening the view for the first time.
  const [refreshedAt, setRefreshedAt] = useState(0);
  // Half of a mouse report, held over until the rest of it arrives.
  const pendingMouse = useRef("");
  const [hoverPlayer, setHoverPlayer] = useState<string | null>(null);
  const [hoverTab, setHoverTab] = useState<ViewName | null>(null);
  const [showHelp, setShowHelp] = useState(previewHelp === true);
  const [filter, setFilter] = useState(previewFilter ?? "");
  const [filtering, setFiltering] = useState(false);
  const [bridge, setBridge] = useState<Bridge | null>(null);
  const [cursor, setCursor] = useState(0);
  const [tick, setTick] = useState(0);
  // Ink's own size, not process.stdout: under a fake terminal (a story, a
  // test) they are different objects and only Ink's knows the real width.
  // Piped or redirected, stdin cannot go into raw mode. Ink throws rather than
  // degrade, so ask first: without a keyboard the board still renders, it just
  // does not take keys.
  const { isRawModeSupported } = useStdin();
  const { stdout } = useStdout();
  const { columns, rows: termRows } = useWindowSize();
  const width = columns || stdout.columns || 120;
  const height = termRows || stdout.rows || 40;

  useEffect(() => {
    if (preview) return;
    const bridge = new Bridge(root, {
      onBoard: (b) => {
        setBoard(b);
        setLastAt(Date.now());
      },
      onStatus: (s, detail) => {
        setConn(s);
        setConnDetail(detail ?? "");
      },
    });
    bridge.start();
    setBridge(bridge);
    return () => {
      bridge.stop();
      setBridge(null);
    };
  }, [root, preview]);

  useEffect(() => {
    // Drives the spinner and the "3s ago" reading. It redraws; it never fetches.
    const timer = setInterval(() => setTick((t) => t + 1), 250);
    return () => clearInterval(timer);
  }, []);

  // The alternate screen, the mouse: both are terminal modes rather than React
  // things, both have to be turned off again on every exit path. The screen
  // goes first so the app owns row 1, which is what makes an absolute mouse
  // row line up with the row the layout computed.
  useEffect(() => {
    if (preview || !stdout.isTTY) return;
    return enterAltScreen((data) => stdout.write(data));
  }, [preview, stdout]);

  useEffect(() => {
    if (preview || !isRawModeSupported || !stdout.isTTY) return;
    return enableMouse((data) => stdout.write(data));
  }, [preview, isRawModeSupported, stdout]);

  // Ink repaints in place, so a terminal that got narrower leaves the old wider
  // frame behind it as litter. Clear the screen and the scrollback on the first
  // frame at a new size. Borrowed from Backboard-R-CLI's resize stabiliser.
  const lastSize = useRef(`${width}x${height}`);
  useLayoutEffect(() => {
    const key = `${width}x${height}`;
    if (lastSize.current === key) return;
    lastSize.current = key;
    if (stdout.isTTY) {
      // Home and erase. On the alternate buffer there is no scrollback to
      // clear, which is the whole point of being on it.
      stdout.write("\u001b[H\u001b[2J");
    }
  }, [width, height, stdout]);

  const rows = useMemo(
    () => matching(ordered(board, settings.enemies, sort), filter),
    [board, settings.enemies, sort, filter],
  );

  // The sidebar takes the right of the frame when there is room for it, and
  // the layout needs to know that before it places anything.
  const wide = width >= 108 && (settings.detail || settings.session);

  const zones = useMemo(() => {
    const teams = board?.teams ?? {};
    const selfTeam = board?.selfTeam ?? "Blue";
    const other = Object.keys(teams).find((t) => t !== selfTeam);
    const ally = matching(arrange(arr(teams[selfTeam]), sort), filter);
    const enemy =
      settings.enemies && board?.state === "INGAME" && other
        ? matching(arrange(arr(teams[other]), sort), filter)
        : [];
    return boardLayout({
      hasMeta: num(board?.winProb) !== null || rrFlow(board?.session?.points).length > 0,
      tabs: VIEWS.map((v) => ({
        key: v.key,
        digit: v.digit,
        text: width >= 96 ? v.label : width >= 62 ? v.short : "",
      })),
      teams: [
        { puuids: ally.map((p) => p.puuid ?? "") },
        { puuids: enemy.map((p) => p.puuid ?? "") },
      ],
      width,
      bodyWidth: wide ? width - SIDEBAR - 3 : width,
    });
  }, [board, settings.enemies, sort, filter, width, wide]);

  const selectedPlayer = rows.find((p) => p.puuid === selected) ?? null;
  const connected = conn === "live";

  // The encounter counts and the last match change when the match does, and at
  // no other time: a lobby is recorded once, by id. Keying those two on the id
  // means a view left open updates itself when the lobby changes, instead of
  // showing whatever happened to be true when it was opened.
  //
  // The last match is a local read. The encounter counts are not, quite: the
  // backend backfills them from a career every ten minutes per account, so a
  // lobby that outlasts the throttle costs one round trip to Riot. That is
  // once per ten minutes at worst, for the view whose whole purpose is to be
  // current, and the rate limiter is in front of it either way.
  //
  // The session and a career are dearer. Asking for a session starts the
  // history enrichment, and a career is per account with no throttle at all,
  // so those two stay on the manual refresh where the cost is one you asked
  // for.
  const matchKey = board?.matchId ?? "none";

  // key === null means "do not ask". Nothing here polls: a view that is not
  // open never costs a request, which is what keeps the Riot side quiet.
  const careerKey = view === "career" && selected ? `${selected}:${refreshedAt}` : null;
  const career = useRequest<Career>(bridge, connected, "profile", careerKey, {
    puuid: selected ?? "",
  });
  const perf = useRequest<Performance>(
    bridge,
    connected,
    "performance",
    view === "session" ? `session:${refreshedAt}` : null,
  );
  const recap = useRequest<Recap>(
    bridge,
    connected,
    "recap",
    view === "recap" ? `recap:${refreshedAt}:${matchKey}` : null,
  );
  const encounters = useRequest<Encounters>(
    bridge,
    connected,
    "encounters",
    view === "encounters" ? `encounters:${refreshedAt}:${matchKey}` : null,
  );

  const canned = <T,>(name: string, live: Fetched<T>): Fetched<T> => {
    const seed = previewApi?.[name];
    if (seed === undefined) return live;
    return { phase: "ready", data: seed as T, error: "" };
  };

  useEffect(() => {
    if (rows.length && !rows.some((p) => p.puuid === selected)) {
      setSelected(rows.find((p) => p.isSelf)?.puuid ?? rows[0]?.puuid ?? null);
    }
  }, [rows, selected]);

  const update = (next: prefs.Settings): void => {
    setSettings(next);
    prefs.save(root, next);
  };

  const handleKey = (input: string, key: Key): void => {
    if (showSettings) {
      if (key.escape || input === ",") {
        setShowSettings(false);
        return;
      }
      if (key.upArrow || input === "k") setCursor((c) => Math.max(0, c - 1));
      if (key.downArrow || input === "j")
        setCursor((c) => Math.min(prefs.OPTIONS.length - 1, c + 1));
      const opt = prefs.OPTIONS[cursor];
      if (!opt) return;
      if (key.return || key.leftArrow || key.rightArrow || input === " ") {
        update({ ...settings, [opt.key]: !settings[opt.key] } as prefs.Settings);
      }
      return;
    }

    // A read can hold several reports or half of one. Whatever it holds, it
    // must not reach the keys below: every report starts with the escape byte,
    // and Escape on the board quits.
    const chunk = parseMouseChunk(pendingMouse.current, input);
    pendingMouse.current = chunk.pending;
    if (chunk.events.length) {
      let wheel = 0;
      let last = null as (typeof chunk.events)[number] | null;
      let press = null as (typeof chunk.events)[number] | null;
      for (const one of chunk.events) {
        if (one.kind === "wheel-up") wheel -= 1;
        else if (one.kind === "wheel-down") wheel += 1;
        else {
          last = one;
          if (one.kind === "press") press = one;
        }
      }

      if (wheel !== 0) {
        if (view === "encounters" || view === "career") {
          setOffset((o) => Math.max(0, o + wheel));
        } else {
          const at = rows.findIndex((p) => p.puuid === selected);
          const next = Math.max(0, Math.min(rows.length - 1, (at < 0 ? 0 : at) + wheel));
          setSelected(rows[next]?.puuid ?? selected);
        }
      }

      // A click is acted on where it was released, and the pointer state comes
      // from wherever it ended up, which is the last report in the read.
      const aim = press ?? last;
      if (aim) {
        const overTab = hitTest(zones.tabs, aim.column, aim.row);
        const overPlayer = view === "board" ? hitTest(zones.players, aim.column, aim.row) : null;
        if (press) {
          if (overTab) {
            setView(overTab);
            setOffset(0);
          } else if (overPlayer) {
            setSelected(overPlayer);
          }
        } else {
          // Only re-render when the thing under the pointer actually changed;
          // motion reporting fires once per cell and would otherwise repaint
          // constantly for nothing.
          if (overTab !== hoverTab) setHoverTab(overTab);
          if (overPlayer !== hoverPlayer) setHoverPlayer(overPlayer);
        }
      }
      return;
    }
    if (chunk.mouse) return;

    // Filter mode swallows everything: while it is on, keys are a search
    // term, not commands. lazygit and k9s both work this way and it is what
    // anyone will try first.
    if (filtering) {
      if (key.escape) {
        setFilter("");
        setFiltering(false);
        return;
      }
      if (key.return) {
        setFiltering(false);
        return;
      }
      if (key.backspace || key.delete) {
        setFilter((f) => f.slice(0, -1));
        return;
      }
      if (input && !key.ctrl && !key.meta) {
        setFilter((f) => f + input);
      }
      return;
    }

    if (input === "/") {
      setFiltering(true);
      return;
    }

    if (input === "q" || (key.ctrl && input === "c")) {
      exit();
      return;
    }
    // Escape backs out: a filter, then the help, then the view. On the board
    // it does nothing at all, deliberately.
    if (key.escape) {
      if (filter) {
        setFilter("");
        return;
      }
      if (showHelp) {
        setShowHelp(false);
        return;
      }

      if (view !== "board") {
        setView("board");
        return;
      }
      // Escape used to quit from here, and it closed the app by accident. The
      // terminal reports the mouse as escape sequences, and a read boundary
      // landing just after the escape byte delivers it alone, which is
      // indistinguishable from the key. Quitting is q, which no sequence can
      // spell.
      return;
    }
    const digit = VIEWS.find((v) => v.digit === input);
    if (digit) {
      setView(digit.key);
      setOffset(0);
      return;
    }
    if (key.tab) {
      const at = VIEWS.findIndex((v) => v.key === view);
      const next = VIEWS[(at + (key.shift ? VIEWS.length - 1 : 1)) % VIEWS.length];
      setView(next?.key ?? "board");
      return;
    }
    if (key.return && view === "board" && selected) {
      setView("career");
      return;
    }
    if (input === "?") {
      setShowHelp((on) => !on);
      return;
    }
    if (input === "r") {
      setRefreshedAt((n) => n + 1);
      return;
    }
    if (input === "s") {
      // Cycle the board order. The team header says which one is active,
      // because a reordered scoreboard with no label is just confusing.
      setSort((current) => {
        const at = SORT_MODES.indexOf(current);
        return SORT_MODES[(at + 1) % SORT_MODES.length] ?? "party";
      });
      return;
    }
    if (input === "g" || key.pageUp) {
      if (view === "encounters" || view === "career") {
        setOffset(0);
        return;
      }
      setSelected(rows[0]?.puuid ?? selected);
      return;
    }
    if (input === "G" || key.pageDown) {
      setSelected(rows[rows.length - 1]?.puuid ?? selected);
      return;
    }
    if (input === ",") {
      setShowSettings(true);
      return;
    }
    if (input === "d") update({ ...settings, detail: !settings.detail });
    if (key.upArrow || input === "k" || key.downArrow || input === "j") {
      const step = key.upArrow || input === "k" ? -1 : 1;
      if (view === "encounters" || view === "career") {
        setOffset((o) => Math.max(0, o + step));
        return;
      }
      const at = rows.findIndex((p) => p.puuid === selected);
      const next = Math.max(0, Math.min(rows.length - 1, (at < 0 ? 0 : at) + step));
      setSelected(rows[next]?.puuid ?? selected);
    }
  };

  // Rendered only when the terminal can give us keys. Ink's useInput reaches
  // for raw mode the moment it mounts and throws when it cannot have it, so
  // piping the scoreboard anywhere used to kill it outright.
  const keys = isRawModeSupported ? <Keys onKey={handleKey} /> : null;

  if (width < MIN_WIDTH) {
    return (
      <>
        {keys}
        <Box flexDirection="column" paddingX={1}>
          <Text bold color={C.bone}>
            Terminal too narrow
          </Text>
          <Text color={C.dim}>
            {`The scoreboard needs ${MIN_WIDTH} columns; this one has ${width}.`}
          </Text>
        </Box>
      </>
    );
  }

  if (showHelp) {
    return (
      <>
        {keys}
        <HelpView width={width} />
      </>
    );
  }

  if (showSettings) {
    return (
      <>
        {keys}
        <SettingsView settings={settings} cursor={cursor} />
      </>
    );
  }

  const current = board ?? {};
  if (!rows.length) {
    return (
      <Box flexDirection="column">
        {keys}
        <Header
          board={current}
          conn={conn}
          lastAt={lastAt}
          now={Date.now()}
          width={width}
          filter={filter}
          filtering={filtering}
        />
        <Holding board={current} conn={conn} detail={connDetail} tick={tick} animate={!preview} />
      </Box>
    );
  }

  const bodyWidth = wide ? width - SIDEBAR - 3 : width - 2;
  const cols = visibleColumns(bodyWidth);
  // Window minus the header, the tab strip, its rule and the footer.
  const hasMeta = num(current.winProb) !== null || rrFlow(current.session?.points).length > 0;
  const bodyHeight = Math.max(1, height - headerHeight(hasMeta) - 3);
  const teams = current.teams ?? {};
  const selfTeam = current.selfTeam ?? "Blue";
  const other = Object.keys(teams).find((t) => t !== selfTeam);
  const player = selectedPlayer;

  const viewHeight = Math.max(4, height - headerHeight(true) - 3);

  if (view !== "board") {
    return (
      <Box flexDirection="column">
        {keys}
        <Header
          board={current}
          conn={conn}
          lastAt={lastAt}
          now={Date.now()}
          width={width}
          filter={filter}
          filtering={filtering}
        />
        <Tabs active={view} width={width} hovered={hoverTab} />
        <Box flexDirection="column" height={viewHeight} overflow="hidden">
          {view === "career" ? (
            <CareerView
              player={player}
              state={canned("profile", career)}
              width={width}
              height={viewHeight}
              offset={offset}
            />
          ) : null}
          {view === "session" ? (
            <SessionView state={canned("performance", perf)} width={width} height={viewHeight} />
          ) : null}
          {view === "recap" ? (
            <RecapView state={canned("recap", recap)} width={width} height={viewHeight} />
          ) : null}
          {view === "encounters" ? (
            <EncountersView
              state={canned("encounters", encounters)}
              width={width}
              height={viewHeight}
              offset={offset}
            />
          ) : null}
        </Box>
        <Box paddingX={1}>
          <Text color={C.faint}>
            <KeyHints
              pairs={[
                ["1-5", "Views"],
                ["Tab", "Next"],
                ["↑↓", "Scroll"],
                ["R", "Refresh"],
                ["Esc", "Back"],
                [",", "Settings"],
                ["Q", "Quit"],
              ]}
            />
          </Text>
        </Box>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" height={height} overflow="hidden">
      {keys}
      <Header
        board={current}
        conn={conn}
        lastAt={lastAt}
        now={Date.now()}
        width={width}
        filter={filter}
        filtering={filtering}
      />
      <Tabs active={view} width={width} hovered={hoverTab} />
      <Box height={bodyHeight} overflow="hidden">
        <Box flexDirection="column" width={bodyWidth} flexShrink={0} overflowX="hidden">
          <TeamBlock
            label="ALLIES"
            color={C.ally}
            players={matching(arrange(arr(teams[selfTeam]), sort), filter)}
            stats={current.teamStats?.[selfTeam]}
            cols={cols}
            selected={selected}
            hovered={hoverPlayer}
            width={bodyWidth}
            sort={sort}
          />
          {settings.enemies && current.state === "INGAME" && other ? (
            <TeamBlock
              label="ENEMIES"
              color={C.enemy}
              players={matching(arrange(arr(teams[other]), sort), filter)}
              stats={current.teamStats?.[other]}
              cols={cols}
              selected={selected}
              hovered={hoverPlayer}
              width={bodyWidth}
              sort={sort}
            />
          ) : null}
        </Box>
        {wide ? (
          <Box flexDirection="column" marginLeft={2} flexShrink={0}>
            {current.state === "PREGAME" ? (
              <TeamComp players={arrange(arr(teams[selfTeam]), sort)} board={current} />
            ) : null}
            {settings.detail ? <Detail p={player} /> : null}
            {settings.session ? <Session board={current} /> : null}
          </Box>
        ) : null}
      </Box>
      <Box paddingX={1}>
        <Text wrap="truncate" color={C.faint}>
          <KeyHints
            pairs={[
              ["↑↓", "Select"],
              ["Enter", "Career"],
              ["1-5", "Views"],
              ["S", "Sort"],
              ["/", "Filter"],
              ["?", "Keys"],
              [",", "Settings"],
              ["Q", "Quit"],
            ]}
          />
        </Text>
      </Box>
    </Box>
  );
}
