import { EventEmitter } from "node:events";
import { render } from "ink";
import { App } from "./app.js";
import { findStory, STORIES, type Story } from "./stories.js";

// Dev only. Renders named stories of the real component tree into a fake
// terminal, so every screen can be looked at and diffed without a TTY.
//   node preview.mjs              every story
//   node preview.mjs ingame       one story
//   node preview.mjs narrow --raw keep the ANSI colour
// Never bundled into dist/overseer.js.

class FakeStdout extends EventEmitter {
  rows = 60;
  isTTY = true;
  frames: string[] = [];
  constructor(public columns: number) {
    super();
  }
  write(chunk: string): boolean {
    this.frames.push(chunk);
    return true;
  }
}

class FakeStdin extends EventEmitter {
  // isTTY false keeps useInput inactive, which is what a non-terminal run does.
  isTTY = false;
  setEncoding(): void {}
  read(): null {
    return null;
  }
  resume(): void {}
  pause(): void {}
  ref(): void {}
  unref(): void {}
}

// Must include the escape byte. Without it this also strips ordinary
// bracketed text, such as the "[,]" key hints in the footer.
const ANSI = new RegExp(`${String.fromCharCode(27)}\\[[0-9;?]*[A-Za-z]`, "g");

async function renderStory(story: Story, raw: boolean): Promise<string> {
  const stdout = new FakeStdout(story.width);
  const instance = render(
    <App
      root={process.cwd()}
      preview={story.board}
      previewSettings={story.settings}
      previewOpenSettings={story.openSettings}
      previewView={story.view}
      previewHelp={story.help}
      previewFilter={story.filter}
      previewSort={story.sort}
      previewApi={story.api}
    />,
    {
      stdout: stdout as never,
      stdin: new FakeStdin() as never,
      exitOnCtrlC: false,
      patchConsole: false,
    },
  );
  await new Promise((resolve) => setTimeout(resolve, 90));
  // Take the frame before unmounting: Ink's final write is cursor cleanup.
  const frame =
    [...stdout.frames].reverse().find((f) => f.replace(ANSI, "").trim().length > 40) ?? "";
  instance.unmount();
  return raw ? frame : frame.replace(ANSI, "");
}

const args = process.argv.slice(2);
const raw = args.includes("--raw");
const quiet = args.includes("--quiet");
const bare = args.includes("--bare");
const wanted = args.filter((a) => !a.startsWith("--"));

if (args.includes("--list")) {
  for (const story of STORIES) {
    console.log(`${story.name.padEnd(12)} ${String(story.width).padStart(4)}  ${story.summary}`);
  }
  process.exit(0);
}

const chosen: Story[] = wanted.length
  ? wanted.map((name) => {
      const story = findStory(name);
      if (!story) {
        console.error(`unknown story: ${name}`);
        process.exit(2);
      }
      return story;
    })
  : STORIES;

const ERROR_FRAME = /ERROR/;
const LOCATION = /:[0-9]+:[0-9]+/;

let failed = 0;
let crashed = 0;
for (const story of chosen) {
  const frame = await renderStory(story, raw);
  const clean = frame.replace(ANSI, "").trim();
  // Ink catches a render fault and draws it into the frame instead of throwing,
  // so the process exits 0 and a story that crashed is indistinguishable from
  // one that rendered. That is how a TypeError in a career reached a release.
  const threw = ERROR_FRAME.test(clean) && LOCATION.test(clean);
  const ok = clean.length > 40 && !threw;
  if (threw) {
    crashed += 1;
    console.error(`${story.name} threw while rendering:`);
    console.error(clean.slice(0, 300));
  } else if (!ok) {
    failed += 1;
  }
  // --quiet is for the lint gate: render everything, print nothing unless a
  // story comes back empty.
  if (quiet) continue;
  // --bare prints the frame and nothing else, for screenshotting.
  if (bare) {
    console.log(frame);
    continue;
  }
  console.log(`\n${"─".repeat(Math.min(story.width, 100))}`);
  console.log(`${story.name}  (${story.width} cols)  ${story.summary}${ok ? "" : "   [EMPTY]"}`);
  console.log("─".repeat(Math.min(story.width, 100)));
  console.log(frame);
}

if (crashed) {
  console.error(`${crashed} story/stories threw while rendering`);
  process.exit(1);
}
if (failed) {
  console.error(`\n${failed} story/stories rendered empty`);
  process.exit(1);
}
process.exit(0);
