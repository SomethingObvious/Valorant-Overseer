// One bundled file so a user needs node and nothing else: no npm install on
// their machine, no node_modules in the release, no resolver to go wrong.
import { build } from "esbuild";

const BANNER = [
  "#!/usr/bin/env node",
  // Some transitive deps are still CommonJS and call require() for node
  // builtins. ESM output has no require, so hand them a real one.
  'import { createRequire as __createRequire } from "node:module";',
  "const require = __createRequire(import.meta.url);",
].join("\n");

await build({
  entryPoints: ["src/main.tsx"],
  outfile: "dist/overseer.js",
  bundle: true,
  platform: "node",
  target: "node20",
  format: "esm",
  jsx: "automatic",
  legalComments: "none",
  minify: false,
  // ink imports react-devtools-core at load but only calls it when DEV=true,
  // which this build never sets. The real package is megabytes of debugger.
  alias: { "react-devtools-core": "./src/devtools-stub.ts" },
  banner: { js: BANNER },
});

console.log("bundled dist/overseer.js");
