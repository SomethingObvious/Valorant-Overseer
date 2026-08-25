import { Text, useAnimation } from "ink";
import type React from "react";

// A highlight that sweeps across text, brightest at its core and falling off
// either side. Adapted from the shadowed spinner in Backboard-R-CLI, which
// blanks the core for a cut-out look; a scoreboard heading reads better as a
// shine, so the core is the brightest tone instead of a hole.
//
// This is the only animation in the app and it runs on one line of text. It
// costs a redraw every 140ms and never touches the bridge or Riot.

const INTERVAL_MS = 140;
const GAP_FRAMES = 4;
const WIDTH = 6;

export type Tone = "base" | "trail" | "core" | "lead";

interface Range {
  start: number;
  end: number;
}

interface Segment {
  key: number;
  text: string;
  tone: Tone;
}

function positiveModulo(value: number, modulus: number): number {
  return ((value % modulus) + modulus) % modulus;
}

/** Where the highlight sits this frame, or null while it is off the end. */
export function shimmerRange(length: number, frame: number): Range | null {
  if (length === 0) return null;
  const cycle = length + GAP_FRAMES + WIDTH;
  const start = positiveModulo(frame, cycle) - WIDTH;
  if (start >= length) return null;
  return { start, end: Math.min(start + WIDTH, length) };
}

function toneAt(index: number, range: Range | null): Tone {
  if (!range) return "base";
  const offset = index - range.start;
  if (offset < 0 || index >= range.end) return "base";
  if (offset <= 1) return "trail";
  if (offset <= 3) return "core";
  if (offset <= 5) return "lead";
  return "base";
}

/** Runs of equal tone, so one <Text> is emitted per run rather than per char. */
export function shimmerSegments(text: string, range: Range | null): Segment[] {
  const segments: Segment[] = [];
  for (let i = 0; i < text.length; i += 1) {
    const tone = toneAt(i, range);
    const char = text[i] ?? "";
    const last = segments[segments.length - 1];
    if (last && last.tone === tone) {
      last.text += char;
      continue;
    }
    segments.push({ key: i, text: char, tone });
  }
  return segments;
}

export interface ShimmerProps {
  text: string;
  tones: Record<Tone, string>;
  bold?: boolean | undefined;
  /** Off means render flat, for reduced motion or a non-interactive render. */
  active?: boolean | undefined;
}

export function Shimmer({ text, tones, bold, active = true }: ShimmerProps): React.ReactElement {
  const { frame } = useAnimation({ interval: INTERVAL_MS });
  const range = active ? shimmerRange(text.length, frame) : null;
  const segments = shimmerSegments(text, range);
  return (
    <Text>
      {segments.map((segment) => (
        <Text key={segment.key} bold={bold === true} color={tones[segment.tone]}>
          {segment.text}
        </Text>
      ))}
    </Text>
  );
}
