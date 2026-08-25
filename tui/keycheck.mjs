// Dev only: bundles src/keycheck.tsx and runs it.

import { spawnSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { build } from "esbuild";

const out = path.join(mkdtempSync(path.join(tmpdir(), "ovkeys-")), "keycheck.js");
await build({
  entryPoints: ["src/keycheck.tsx"],
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
const r = spawnSync(process.execPath, [out, ...process.argv.slice(2)], { stdio: "inherit" });
process.exit(r.status ?? 1);
