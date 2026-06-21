import solidPlugin from "@opentui/solid/bun-plugin";

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

console.log("Bundled successfully to ./dist-bundle/cli.js");
