// Braille cells give 2 columns x 4 rows of dots per character, so a chart drawn
// this way has four times the vertical resolution of block characters and twice
// the horizontal. btop does the same thing, and it is the difference between a
// session graph you can read a trend off and eight blocks that all look alike.
//
//   dot bits, per the Unicode braille block starting at U+2800:
//     1 4      0x01 0x08
//     2 5      0x02 0x10
//     3 6      0x04 0x20
//     7 8      0x40 0x80

const BRAILLE_BASE = 0x2800;
const DOTS: number[][] = [
  [0x01, 0x08],
  [0x02, 0x10],
  [0x04, 0x20],
  [0x40, 0x80],
];

export interface Column {
  /** 0..1 of the chart height. Values outside are clamped. */
  value: number;
  /** Chosen per column so gains and losses can be coloured differently. */
  positive: boolean;
}

export interface ChartRow {
  /** The row's place in the chart, which is its identity. */
  key: string;
  text: string;
  /** One entry per character in `text`, true when that cell holds a gain. */
  positives: boolean[];
}

/**
 * Renders columns as vertical braille bars growing up from the bottom.
 * Two data columns share one character cell.
 */
export function brailleBars(columns: Column[], rows: number): ChartRow[] {
  const height = Math.max(1, rows);
  const dotRows = height * 4;
  const cells = Math.ceil(columns.length / 2);

  const out: ChartRow[] = [];
  for (let row = 0; row < height; row += 1) {
    let text = "";
    const positives: boolean[] = [];
    for (let cell = 0; cell < cells; cell += 1) {
      let bits = 0;
      let anyPositive = false;
      for (let half = 0; half < 2; half += 1) {
        const column = columns[cell * 2 + half];
        if (!column) continue;
        const filled = Math.round(Math.max(0, Math.min(1, column.value)) * dotRows);
        if (filled > 0 && column.positive) anyPositive = true;
        for (let dot = 0; dot < 4; dot += 1) {
          // Row 0 is the top of the chart; bars grow from the bottom up.
          const dotFromBottom = dotRows - (row * 4 + dot) - 1;
          if (dotFromBottom < filled) {
            bits |= DOTS[dot]?.[half] ?? 0;
          }
        }
      }
      text += String.fromCharCode(BRAILLE_BASE + bits);
      positives.push(anyPositive);
    }
    out.push({ key: `row${row}`, text, positives });
  }
  return out;
}

/**
 * Splits a row into runs of equal colour so the caller emits one span per run
 * instead of one per character.
 */
export function colourRuns(row: ChartRow): Array<{ key: string; text: string; positive: boolean }> {
  const runs: Array<{ key: string; text: string; positive: boolean }> = [];
  for (let i = 0; i < row.text.length; i += 1) {
    const char = row.text[i] ?? "";
    const positive = row.positives[i] ?? true;
    const last = runs[runs.length - 1];
    if (last && last.positive === positive) {
      last.text += char;
      continue;
    }
    runs.push({ key: `${row.key}:${i}`, text: char, positive });
  }
  return runs;
}
