// Dev-only: bundles src/preview.tsx to a temp file and runs it, so the real
// component tree can be rendered and looked at without a terminal.

import { spawnSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { build } from "esbuild";

const out = path.join(mkdtempSync(path.join(tmpdir(), "ovpreview-")), "preview.js");
await build({
  entryPoints: ["src/preview.tsx"],
  outfile: out,
  bundle: true,
  platform: "node",
  target: "node20",
  format: "esm",
  jsx: "automatic",
  alias: { "react-devtools-core": "./src/devtools-stub.ts" },
  banner: {
    js: [
      'import { createRequire as __cr } from "node:module";',
      "const require = __cr(import.meta.url);",
    ].join("\n"),
  },
});
const result = spawnSync(process.execPath, [out, ...process.argv.slice(2)], {
  stdio: "inherit",
});
// Propagate the child's status, or a failing story is invisible to the caller.
process.exit(result.status ?? 1);
