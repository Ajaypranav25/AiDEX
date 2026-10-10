import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    mockReset: true,
    restoreMocks: true,
    unstubGlobals: true,
    coverage: {
      include: ["src/**/*.ts"],
      // DTOs are type declarations and label tables.
      exclude: ["src/**/*.test.ts", "src/dto/**"],
      thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
    },
  },
});
