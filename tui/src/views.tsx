import { Box, Text, useAnimation } from "ink";
import type React from "react";
import type {
  Career,
  Encounters,
  Fetched,
  Performance,
  Recap,
  RecapPlayer,
  SplitRow,
} from "./api.js";
import { brailleBars, colourRuns } from "./chart.js";
import { arr, bar, dash, kd2, NONE, num, pad, trim } from "./format.js";
import { C, kdColor } from "./theme.js";
import type { Player } from "./types.js";

/**
 * A bordered panel costs two lines of border and one of title, so a panel
 * showing n rows is n + 3 lines tall.
 */
export const PANEL_CHROME = 3;

/**
 * How many rows fit in the height a view was given. Scrolling is a last resort
 * here: a view that can shrink to fit should, and only a genuinely unbounded
 * list -- the encounter log runs to hundreds of accounts -- still needs it.
 */
export function fitRows(height: number, chrome = PANEL_CHROME, min = 1): number {
  return Math.max(min, height - chrome);
}

/** A window into a list, plus the text that says where you are in it. */
export function windowOf<T>(
  items: T[],
  offset: number,
  size: number,
): { rows: T[]; label: string } {
  const page = Math.max(1, size);
  if (items.length <= page) {
    return { rows: items, label: `${items.length}` };
  }
  const start = Math.max(0, Math.min(offset, items.length - page));
  return {
    rows: items.slice(start, start + page),
    label: `${start + 1}-${start + page} of ${items.length}`,
  };
}

export type ViewName = "board" | "career" | "session" | "recap" | "encounters";

export const VIEWS: Array<{ key: ViewName; label: string; short: string; digit: string }> = [
  { key: "board", label: "BOARD", short: "BOARD", digit: "1" },
  { key: "career", label: "CAREER", short: "CAREER", digit: "2" },
  { key: "session", label: "SESSION", short: "SESSION", digit: "3" },
  { key: "recap", label: "LAST MATCH", short: "MATCH", digit: "4" },
  { key: "encounters", label: "SEEN BEFORE", short: "SEEN", digit: "5" },
];

export function Tabs({
  active,
  width,
  hovered,
}: {
  active: ViewName;
  width: number;
  hovered?: ViewName | null | undefined;
}): React.ReactElement {
  return (
    <Box flexDirection="column">
      <Box paddingX={1} width={width} flexShrink={0} overflowX="hidden">
        {VIEWS.map((view) => {
          const on = view.key === active;
          const over = view.key === hovered;
          // Long labels wrapped the strip onto a second line on a narrow
          // terminal; below 96 columns the names get shorter, below 62 they go.
          const text = width >= 96 ? view.label : width >= 62 ? view.short : "";
          const body = text ? ` ${view.digit} ${text} ` : ` ${view.digit} `;
          return (
            <Box key={view.key} marginRight={1} flexShrink={0}>
              {on ? (
                <Text bold wrap="truncate" color={C.ink} backgroundColor={C.red}>
                  {body}
                </Text>
              ) : (
                <Text wrap="truncate" bold={over} color={over ? C.bone : C.faint}>
                  {body}
                </Text>
              )}
            </Box>
          );
        })}
      </Box>
      <Rule width={width} />
    </Box>
  );
}

/** A hairline the full width of the terminal, used to separate the chrome. */
export function Rule({ width }: { width: number }): React.ReactElement {
  return <Text color={C.line}>{"─".repeat(Math.max(1, width))}</Text>;
}

function Panel({
  title,
  children,
  width,
}: {
  title: string;
  children: React.ReactNode;
  width?: number;
}): React.ReactElement {
  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={C.line}
      paddingX={1}
      width={width}
      marginBottom={1}
      flexShrink={0}
    >
      <Box>
        <Text color={C.red}>{"▍"}</Text>
        <Text bold color={C.dim}>
          {title}
        </Text>
      </Box>
      {children}
    </Box>
  );
}

// A request that says "Fetching." and then sits there looks like a hang. The
// dots move so it is obvious something is still happening.
function Fetching(): React.ReactElement {
  const { frame } = useAnimation({ interval: 180 });
  const lit = frame % 4;
  return (
    <Box paddingX={2} paddingY={1}>
      <Text color={C.dim}>Fetching</Text>
      {[0, 1, 2].map((i) => (
        <Text key={i} bold color={i < lit ? C.gold : C.line}>
          .
        </Text>
      ))}
    </Box>
  );
}

/** Shared empty/loading/error body so every view fails the same way. */
export function Status<T>({
  state,
  empty,
  children,
}: {
  state: Fetched<T>;
  empty: string;
  children: (data: T) => React.ReactElement;
}): React.ReactElement {
  if (state.phase === "loading") {
    return <Fetching />;
  }
  if (state.phase === "error") {
    return (
      <Box flexDirection="column" paddingX={2} paddingY={1}>
        <Text color={C.loss}>{`Couldn't load: ${state.error}.`}</Text>
        <Text color={C.faint}>The backend answers this once VALORANT is readable.</Text>
      </Box>
    );
  }
  if (state.phase === "idle" || !state.data) {
    return (
      <Box paddingX={2} paddingY={1}>
        <Text color={C.faint}>{empty}</Text>
      </Box>
    );
  }
  return children(state.data);
}

function SplitTable({
  title,
  rows,
  label,
  width,
  max = 8,
}: {
  title: string;
  rows: SplitRow[];
  label: (row: SplitRow) => string;
  width: number;
  max?: number | undefined;
}): React.ReactElement | null {
  const list = arr(rows).slice(0, Math.max(1, max));
  if (!list.length) return null;
  return (
    <Panel title={title} width={width}>
      {list.map((row) => {
        const wr = num(row.winRate);
        const games = num(row.games) ?? 0;
        return (
          <Box key={label(row)}>
            <Text color={C.text}>{pad(label(row), Math.max(8, width - 30))}</Text>
            <Text color={C.faint}>{pad(`${games}g`, 5, "right")}</Text>
            <Text color={wr !== null && wr >= 50 ? C.ally : C.loss}>
              {pad(wr === null ? NONE : `${Math.round(wr)}%`, 6, "right")}
            </Text>
            <Text color={kdColor(row.kd)}>
              {pad(row.kd === null || row.kd === undefined ? "" : ` ${kd2(row.kd)}`, 6)}
            </Text>
            <Text color={C.faint}>{bar(wr, 100, 5)}</Text>
          </Box>
        );
      })}
    </Panel>
  );
}

// --- career -----------------------------------------------------------------

export function CareerView({
  player,
  state,
  width,
  height,
  offset,
}: {
  player: Player | null;
  state: Fetched<Career>;
  width: number;
  height: number;
  offset: number;
}): React.ReactElement {
  if (!player) {
    return (
      <Box paddingX={2} paddingY={1}>
        <Text color={C.faint}>Select a player on the board, then press 2.</Text>
      </Box>
    );
  }
  const half = Math.max(30, Math.floor((width - 5) / 2));
  // One line for the name, the splits panel, then the matches, then the
  // encounter note. Whatever is left is how many matches can be listed.
  const splitRows = Math.max(1, Math.min(6, height - 18));
  const matchRows = Math.max(1, height - (splitRows + PANEL_CHROME) - 9);
  return (
    <Box flexDirection="column">
      <Box paddingX={1}>
        <Text bold color={C.bone}>
          {player.name ?? NONE}
        </Text>
        <Text color={player.rankColor ?? C.dim}>{`  ${player.rank ?? NONE}`}</Text>
        <Text color={C.faint}>{`   Peak ${player.peakRank ?? NONE}`}</Text>
        <Text color={C.faint}>{`   Level ${num(player.level) ?? NONE}`}</Text>
      </Box>
      <Status state={state} empty="No career data for this player yet.">
        {(career) => (
          <Box flexDirection="column">
            {career.error ? (
              <Box paddingX={2}>
                <Text color={C.loss}>{career.error}</Text>
              </Box>
            ) : null}
            <Box>
              <SplitTable
                title="AGENTS"
                rows={arr(career.agents)}
                label={(r) => r.agent ?? r.name ?? NONE}
                width={half}
                max={splitRows}
              />
              <Box marginLeft={1}>
                <SplitTable
                  title="MAPS"
                  rows={arr(career.maps)}
                  label={(r) => r.map ?? r.name ?? NONE}
                  width={half}
                  max={splitRows}
                />
              </Box>
            </Box>
            <MatchList
              matches={arr(career.matches)}
              width={half * 2 + 1}
              offset={offset}
              size={matchRows}
            />
            {career.encounter ? (
              <Panel title="SEEN BEFORE" width={half * 2 + 1}>
                <Text color={C.text}>
                  {`${num(career.encounter.seen) ?? num(career.encounter.games) ?? 0} previous matches`}
                  {career.encounter.withYou !== undefined
                    ? `, ${num(career.encounter.withYou) ?? 0} on your team`
                    : ""}
                  {career.encounter.againstYou !== undefined
                    ? `, ${num(career.encounter.againstYou) ?? 0} against you.`
                    : ""}
                </Text>
              </Panel>
            ) : null}
          </Box>
        )}
      </Status>
    </Box>
  );
}

function MatchList({
  matches,
  width,
  offset,
  size,
}: {
  matches: Career["matches"];
  width: number;
  offset: number;
  size: number;
}): React.ReactElement | null {
  const all = arr(matches);
  if (!all.length) return null;
  const page = windowOf(all, offset, size);
  const list = page.rows;
  return (
    <Panel title={`RECENT MATCHES  ${page.label}`} width={width}>
      {list.map((m, i) => {
        const won = (m.result ?? "").toLowerCase().startsWith("v");
        const delta = num(m.delta);
        return (
          <Box key={m.matchId ?? `${i}:${m.map ?? ""}:${m.agent ?? ""}`}>
            <Text color={won ? C.ally : C.loss}>{won ? "W " : "L "}</Text>
            <Text color={C.text}>{pad(m.map ?? NONE, 12)}</Text>
            <Text color={C.ice}>{pad(m.agent ?? NONE, 11)}</Text>
            <Text color={C.faint}>{pad(m.score ?? "", 8)}</Text>
            <Text color={C.text}>
              {pad(`${num(m.kills) ?? 0}/${num(m.deaths) ?? 0}/${num(m.assists) ?? 0}`, 10)}
            </Text>
            <Text color={kdColor(m.kd)}>{pad(kd2(m.kd), 6, "right")}</Text>
            <Text color={C.faint}>{pad(m.acs === null ? "" : ` ${dash(m.acs)} ACS`, 10)}</Text>
            {delta !== null ? (
              <Text color={delta >= 0 ? C.ally : C.loss}>
                {pad(`${delta > 0 ? "+" : ""}${trim(delta)} RR`, 9, "right")}
              </Text>
            ) : null}
          </Box>
        );
      })}
    </Panel>
  );
}

// --- session ----------------------------------------------------------------

export function SessionView({
  state,
  width,
  height,
}: {
  state: Fetched<Performance>;
  width: number;
  height: number;
}): React.ReactElement {
  const half = Math.max(30, Math.floor((width - 5) / 2));
  return (
    <Status state={state} empty="No ranked history recorded yet.">
      {(perf) => {
        const s = perf.summary ?? {};
        const points = arr(perf.points);
        const matches = num(s.matches) ?? 0;
        if (!matches && !points.length) {
          return (
            <Box flexDirection="column" paddingX={2} paddingY={1}>
              <Text color={C.dim}>No competitive matches recorded for this account yet.</Text>
              <Text color={C.faint}>
                History fills in as you play, once the client becomes readable.
              </Text>
            </Box>
          );
        }
        const net = num(s.net) ?? 0;
        // The top row of panels is a fixed eight lines. What is left goes to
        // the splits, and the insights only appear if there is still room, so
        // the view fits its window instead of asking to be scrolled.
        const spare = height - 9;
        const splitRows = Math.max(1, Math.min(8, spare - 6));
        const roomForInsights = spare - (splitRows + PANEL_CHROME) >= 4;
        const peak = Math.max(1, ...points.map((q) => Math.abs(num(q.delta) ?? 0)));
        // Widen each match into sub-columns so a short history still fills the
        // panel instead of sitting in five cells.
        const perMatch = points.length
          ? Math.max(1, Math.min(4, Math.floor(((half - 4) * 2) / points.length)))
          : 1;
        const rows = points.length
          ? brailleBars(
              points.flatMap((p) => {
                const d = num(p.delta) ?? 0;
                return Array.from({ length: perMatch }, () => ({
                  value: Math.abs(d) / peak,
                  positive: d >= 0,
                }));
              }),
              4,
            )
          : [];
        return (
          <Box flexDirection="column">
            <Box>
              <Panel title="RANKED" width={half}>
                <Box>
                  <Text color={C.text}>{pad("Record", 12)}</Text>
                  <Text color={C.ally}>{`${num(s.wins) ?? 0}W`}</Text>
                  <Text color={C.faint}>{" / "}</Text>
                  <Text color={C.loss}>{`${num(s.losses) ?? 0}L`}</Text>
                  <Text color={C.faint}>{`  of ${matches}`}</Text>
                </Box>
                <Box>
                  <Text color={C.text}>{pad("Win rate", 12)}</Text>
                  <Text color={(num(s.winRate) ?? 0) >= 50 ? C.ally : C.loss}>
                    {dash(s.winRate, "%")}
                  </Text>
                </Box>
                <Box>
                  <Text color={C.text}>{pad("Net RR", 12)}</Text>
                  <Text color={net >= 0 ? C.ally : C.loss}>{`${net > 0 ? "+" : ""}${net}`}</Text>
                </Box>
                <Box>
                  <Text color={C.text}>{pad("Avg win", 12)}</Text>
                  <Text color={C.ally}>{dash(s.avgWin)}</Text>
                  <Text color={C.faint}>{"   Avg loss "}</Text>
                  <Text color={C.loss}>{dash(s.avgLoss)}</Text>
                </Box>
                <Box>
                  <Text color={C.text}>{pad("Current", 12)}</Text>
                  <Text color={s.current?.color ?? C.dim}>{s.current?.name ?? NONE}</Text>
                  {s.current?.rr !== null && s.current?.rr !== undefined ? (
                    <Text color={C.faint}>{`  ${s.current.rr} RR`}</Text>
                  ) : null}
                </Box>
              </Panel>
              <Box marginLeft={1}>
                <Panel title="RR PER MATCH" width={half}>
                  {rows.length ? (
                    rows.map((row) => (
                      <Box key={row.key}>
                        {colourRuns(row).map((run) => (
                          <Text key={run.key} color={run.positive ? C.ally : C.loss}>
                            {run.text}
                          </Text>
                        ))}
                      </Box>
                    ))
                  ) : (
                    <Text color={C.faint}>No points yet.</Text>
                  )}
                  <Text color={C.faint}>{`${points.length} matches. `}</Text>
                  <Text>
                    <Text color={C.ally}>{"Green"}</Text>
                    <Text color={C.faint}>{" gained RR, "}</Text>
                    <Text color={C.loss}>{"red"}</Text>
                    <Text color={C.faint}>{" lost it."}</Text>
                  </Text>
                </Panel>
              </Box>
            </Box>
            <Box>
              <SplitTable
                title="BY MAP"
                rows={arr(perf.splits?.maps)}
                label={(r) => r.map ?? r.name ?? NONE}
                width={half}
                max={splitRows}
              />
              <Box marginLeft={1}>
                <SplitTable
                  title="BY AGENT"
                  rows={arr(perf.splits?.agents)}
                  label={(r) => r.agent ?? r.name ?? NONE}
                  width={half}
                  max={splitRows}
                />
              </Box>
            </Box>
            {roomForInsights && arr(perf.insights).length ? (
              <Panel title="INSIGHTS" width={half * 2 + 1}>
                {arr(perf.insights)
                  .slice(0, 5)
                  .map((insight, i) => (
                    <Box key={insight.title ?? insight.text ?? `insight${i}`}>
                      <Text color={C.gold}>{"* "}</Text>
                      <Text color={C.text}>{insight.title ?? insight.text ?? ""}</Text>
                      {insight.detail ? (
                        <>
                          <Text color={C.line}>{"  |  "}</Text>
                          <Text color={C.faint}>{insight.detail}</Text>
                        </>
                      ) : null}
                    </Box>
                  ))}
              </Panel>
            ) : null}
          </Box>
        );
      }}
    </Status>
  );
}

// --- last match -------------------------------------------------------------

// rail + agent + rank + K/D/A + kd + acs + adr + kast + hs + fb. The name gets
// whatever is left, so a row can never outgrow the panel and wrap.
const RECAP_FIXED = 1 + 10 + 13 + 11 + 6 + 6 + 6 + 6 + 5 + 5;
// Everything the round data yields beyond the usual scoreboard: economy,
// opening duels both ways, multikills, clutches, the spike, and the gun. It
// costs nothing to compute and needs 46 more columns to show, so it appears
// when there is room and stays out of the way when there is not.
const RECAP_RICH = 6 + 7 + 5 + 8 + 7 + 11;
const richAt = (width: number): boolean => width >= RECAP_FIXED + RECAP_RICH + 24;
const RECAP_NAME_MAX = 22;

function RecapHead({ width }: { width: number }): React.ReactElement {
  return (
    <Box>
      <Text color={C.faint}> </Text>
      <Text bold color={C.faint}>
        {pad("AGENT", 10)}
        {pad("NAME", Math.min(RECAP_NAME_MAX, Math.max(10, width - RECAP_FIXED)))}
        {pad("RANK", 13)}
        {pad("K / D / A", 11)}
        {pad("K/D", 6, "right")}
        {pad("ACS", 6, "right")}
        {pad("ADR", 6, "right")}
        {pad("KAST", 6, "right")}
        {pad("HS", 5, "right")}
        {pad("FB", 5, "right")}
        {richAt(width) ? (
          <>
            {pad("ECON", 6, "right")}
            {pad("DUELS", 7, "right")}
            {pad("MK", 5, "right")}
            {pad("CLUTCH", 8, "right")}
            {pad("SPIKE", 7, "right")}
            {pad("  WEAPON", 11)}
          </>
        ) : null}
      </Text>
    </Box>
  );
}

function RecapRow({ p, width }: { p: RecapPlayer; width: number }): React.ReactElement {
  const kast = num(p.kast);
  const fb = num(p.firstBloods) ?? 0;
  const fd = num(p.firstDeaths) ?? 0;
  const clutch = num(p.clutches) ?? 0;
  const spike = (num(p.plants) ?? 0) + (num(p.defuses) ?? 0);
  // How many of those actually won the round. A plant that loses is not
  // the same as one that holds, and the total alone cannot say which.
  const spikeWon = (num(p.plantsWon) ?? 0) + (num(p.defusesWon) ?? 0);
  // Every multikill, not one number per size: a 2k and a 4k both count here.
  const multi = Object.values(p.multiKills ?? {}).reduce((n, v) => n + (num(v) ?? 0), 0);
  return (
    <Box>
      <Text color={p.team === "Blue" ? C.ally : C.enemy}>{"▎"}</Text>
      <Text color={C.ice}>{pad(p.agent ?? NONE, 10)}</Text>
      <Text color={C.text}>
        {pad(p.name ?? NONE, Math.min(RECAP_NAME_MAX, Math.max(10, width - RECAP_FIXED)))}
      </Text>
      <Text color={p.rankColor ?? C.dim}>{pad(p.rank ?? NONE, 13)}</Text>
      <Text color={C.text}>
        {pad(`${num(p.kills) ?? 0}/${num(p.deaths) ?? 0}/${num(p.assists) ?? 0}`, 11)}
      </Text>
      <Text color={kdColor(p.kd)}>{pad(kd2(p.kd), 6, "right")}</Text>
      <Text color={C.text}>{pad(dash(p.acs), 6, "right")}</Text>
      <Text color={C.ice}>{pad(dash(p.adr), 6, "right")}</Text>
      <Text color={kast !== null && kast >= 70 ? C.ally : C.dim}>
        {pad(kast === null ? NONE : `${kast}%`, 6, "right")}
      </Text>
      <Text color={C.faint}>{pad(dash(p.hsPct, "%"), 5, "right")}</Text>
      <Text color={fb > 0 ? C.gold : C.faint}>{pad(fb ? String(fb) : NONE, 5, "right")}</Text>
      {richAt(width) ? (
        <>
          <Text color={C.faint}>{pad(dash(p.econ), 6, "right")}</Text>
          <Text color={fb > fd ? C.ally : fd > fb ? C.loss : C.faint}>
            {pad(fb || fd ? `${fb}/${fd}` : NONE, 7, "right")}
          </Text>
          <Text color={multi ? C.gold : C.faint}>
            {pad(multi ? String(multi) : NONE, 5, "right")}
          </Text>
          <Text color={clutch ? C.gold : C.faint}>
            {pad(clutch ? `${clutch}/${clutch + (num(p.clutchesLost) ?? 0)}` : NONE, 8, "right")}
          </Text>
          <Text color={spike && spikeWon === spike ? C.ally : C.faint}>
            {pad(spike ? `${spikeWon}/${spike}` : NONE, 7, "right")}
          </Text>
          <Text color={C.ice}>{pad(`  ${p.topWeapon?.name ?? ""}`, 11)}</Text>
        </>
      ) : null}
    </Box>
  );
}

export function RecapView({
  state,
  width,
  height,
}: {
  state: Fetched<Recap>;
  width: number;
  height: number;
}): React.ReactElement {
  return (
    <Status state={state} empty="No completed match to recap yet.">
      {(recap) => {
        const players = arr(recap.players);
        const scores = recap.scores ?? {};
        const teams = Object.keys(scores);
        const rrDelta = num(recap.rrDelta);
        const you = recap.you;
        const won = (recap.result ?? "").toLowerCase().startsWith("v");
        // The panel takes what the extra columns need when the terminal has
        // it, and stops at the plain scoreboard when it does not, rather than
        // stretching a short row across a wide screen.
        const want = RECAP_FIXED + RECAP_NAME_MAX + 6 + (richAt(width - 2) ? RECAP_RICH : 0);
        const panel = Math.min(width - 2, want);
        return (
          <Box flexDirection="column">
            <Box paddingX={1}>
              <Text bold color={won ? C.ally : C.loss}>
                {won ? "VICTORY" : (recap.result ?? NONE).toUpperCase()}
              </Text>
              <Text bold color={C.bone}>{`  ${recap.map ?? NONE}`}</Text>
              <Text color={C.dim}>{`  ${recap.mode ?? ""}`}</Text>
              {teams.length === 2 ? (
                <>
                  <Text color={C.ally}>{`   ${scores[teams[0] ?? ""] ?? 0}`}</Text>
                  <Text color={C.faint}>:</Text>
                  <Text color={C.enemy}>{`${scores[teams[1] ?? ""] ?? 0}`}</Text>
                </>
              ) : null}
              {rrDelta !== null ? (
                <Text bold color={rrDelta >= 0 ? C.ally : C.loss}>
                  {`   ${rrDelta > 0 ? "+" : ""}${rrDelta} RR`}
                </Text>
              ) : null}
            </Box>
            {you && height >= 18 ? (
              <Box paddingX={1}>
                <Text color={C.faint}>{"you  "}</Text>
                <Text color={C.ice}>{`${you.agent ?? NONE}  `}</Text>
                <Text color={C.text}>
                  {`${num(you.kills) ?? 0}/${num(you.deaths) ?? 0}/${num(you.assists) ?? 0}`}
                </Text>
                <Text color={kdColor(you.kd)}>{`   ${kd2(you.kd)} K/D`}</Text>
                <Text color={C.faint}>
                  {`   ${dash(you.acs)} ACS   ${dash(you.adr)} ADR   ${dash(you.kast, "% KAST")}`}
                </Text>
                <Text color={C.faint}>
                  {`   ${dash(you.hsPct, "% HS")}`}
                  {num(you.firstBloods)
                    ? `   ${num(you.firstBloods)} first blood${num(you.firstBloods) === 1 ? "" : "s"}`
                    : ""}
                  {you.topWeapon?.name
                    ? `   ${you.topWeapon.name} ${num(you.topWeapon.kills) ?? 0}`
                    : ""}
                </Text>
              </Box>
            ) : null}
            {recap.mvp && height >= 20 ? (
              <Box paddingX={1}>
                <Text color={C.gold}>{"MVP  "}</Text>
                <Text color={C.text}>{recap.mvp.name ?? NONE}</Text>
                <Text color={C.faint}>
                  {`  ${recap.mvp.agent ?? ""}  ${dash(recap.mvp.acs)} ACS`}
                </Text>
              </Box>
            ) : null}
            {players.length ? (
              <Panel title="SCOREBOARD" width={panel}>
                <RecapHead width={panel - 4} />
                {players.map((p, i) => (
                  <Box key={p.puuid ?? i} flexDirection="column">
                    {i > 0 && p.team !== players[i - 1]?.team ? (
                      <Text color={C.line}>{"\u2500".repeat(Math.max(1, panel - 4))}</Text>
                    ) : null}
                    <RecapRow p={p} width={panel - 4} />
                  </Box>
                ))}
              </Panel>
            ) : null}
          </Box>
        );
      }}
    </Status>
  );
}

// --- encounters -------------------------------------------------------------

export function EncountersView({
  state,
  width,
  height,
  offset,
}: {
  state: Fetched<Encounters>;
  width: number;
  height: number;
  offset: number;
}): React.ReactElement {
  return (
    <Status state={state} empty="Nobody logged yet - this fills in as you play.">
      {(data) => {
        const rows = arr(data.players);
        if (!rows.length) {
          return (
            <Box flexDirection="column" paddingX={2} paddingY={1}>
              <Text color={C.dim}>No repeat players recorded yet.</Text>
              <Text color={C.faint}>
                Overseer logs everyone you meet, so a name you have seen before shows up here.
              </Text>
            </Box>
          );
        }
        // One line goes to the total underneath the list.
        const page = windowOf(rows, offset, fitRows(height, PANEL_CHROME + 2));
        return (
          <Panel title={`SEEN BEFORE  ${page.label}`}>
            {page.rows.map((row, i) => {
              const wins = num(row.wins) ?? 0;
              const losses = num(row.losses) ?? 0;
              const draws = num(row.draws) ?? 0;
              return (
                <Box key={row.puuid ?? i}>
                  <Text color={C.text}>{pad(row.name ?? NONE, Math.max(14, width - 62))}</Text>
                  <Text color={row.rankColor ?? C.dim}>{pad(row.rank ?? NONE, 13)}</Text>
                  <Text color={C.faint}>
                    {pad(`${num(row.seen) ?? num(row.games) ?? 0}x`, 6, "right")}
                  </Text>
                  <Text color={C.ally}>{pad(`${wins}W`, 5, "right")}</Text>
                  <Text color={C.loss}>{pad(`${losses}L`, 5, "right")}</Text>
                  <Text color={draws ? C.gold : C.faint}>
                    {pad(draws ? `${draws}D` : "", 4, "right")}
                  </Text>
                  <Text color={C.faint}>{`  ${row.lastMap ?? ""}`}</Text>
                </Box>
              );
            })}
            <Box marginTop={1}>
              <Text color={C.faint}>
                {`${num(data.accountCount) ?? rows.length} accounts logged in total`}
              </Text>
            </Box>
          </Panel>
        );
      }}
    </Status>
  );
}
