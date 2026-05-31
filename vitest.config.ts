import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["packages/*/test/**/*.test.ts"],
    environment: "node",
    setupFiles: ["./packages/host-runtime/test/setup.ts"],
  },
});
