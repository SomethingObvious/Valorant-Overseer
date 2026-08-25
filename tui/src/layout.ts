import type { Hit } from "./mouse.js";
import type { ViewName } from "./views.js";

// Where things are on screen, worked out from the same inputs the render uses.
// Ink does not hand back absolute coordinates, so the only way to know which
// row the pointer is over is to compute it -- and the only way to trust that
// computation is to check it against a real frame, which the self-check does.

export interface TabZone {
  key: ViewName;
  digit: string;
  text: string;
}

export interface BoardTeam {
  puuids: string[];
}

export interface BoardLayout {
  tabs: Array<Hit<ViewName>>;
  players: Array<Hit<string>>;
  /** The row the tab strip sits on, exported so the check can find it. */
  tabRow: number;
}

/** One line for the title, and three more when the meta line is showing. */
export function headerHeight(hasMeta: boolean): number {
  return hasMeta ? 4 : 1;
}

export function boardLayout(opts: {
  hasMeta: boolean;
  tabs: TabZone[];
  teams: BoardTeam[];
  width: number;
  /** Where the player rows stop. The sidebar is not part of the table, and a
   *  click in it must not land on the row beside it. */
  bodyWidth?: number | undefined;
}): BoardLayout {
  const { hasMeta, tabs, teams, width } = opts;
  const bodyWidth = opts.bodyWidth ?? width;

  // Rows are 1-based because that is how the terminal reports them.
  const tabRow = headerHeight(hasMeta) + 1;
  const ruleRow = tabRow + 1;

  const tabZones: Array<Hit<ViewName>> = [];
  // The strip starts one column in (paddingX) and each tab is " N LABEL "
  // followed by a one-column gap.
  let column = 2;
  for (const tab of tabs) {
    const label = tab.text ? ` ${tab.digit} ${tab.text} ` : ` ${tab.digit} `;
    tabZones.push({
      top: tabRow,
      height: 1,
      left: column,
      width: label.length,
      value: tab.key,
    });
    column += label.length + 1;
  }

  const players: Array<Hit<string>> = [];
  // Each panel: a top border carrying the title, a column header, the rows,
  // a bottom border, and a blank line after it.
  let row = ruleRow + 1;
  for (const team of teams) {
    if (!team.puuids.length) continue;
    const firstPlayerRow = row + 2;
    team.puuids.forEach((puuid, i) => {
      players.push({
        top: firstPlayerRow + i,
        height: 1,
        left: 1,
        width: bodyWidth,
        value: puuid,
      });
    });
    row = firstPlayerRow + team.puuids.length + 2;
  }

  return { tabs: tabZones, players, tabRow };
}
