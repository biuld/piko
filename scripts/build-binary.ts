import solidPlugin from "@opentui/solid/bun-plugin";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);

console.log("Bundling piko into a single JS file...");

const result = await Bun.build({
  entrypoints: ["./packages/cli/src/cli.ts"],
  outdir: "./dist-bundle",
  target: "bun",
  plugins: [solidPlugin],
  naming: "[name].js",
});

if (!result.success) {
  console.error("Bundle failed:");
  for (const log of result.logs) {
    console.error(log);
  }
  process.exit(1);
}

const opentuiCoreDir = dirname(require.resolve("@opentui/core"));
const workerResult = await Bun.build({
  entrypoints: [join(opentuiCoreDir, "parser.worker.js")],
  outdir: "./dist-bundle",
  target: "bun",
  naming: "[name].[ext]",
});

if (!workerResult.success) {
  console.error("Worker bundle failed:");
  for (const log of workerResult.logs) {
    console.error(log);
  }
  process.exit(1);
}

console.log("Bundled successfully to ./dist-bundle/cli.js");
