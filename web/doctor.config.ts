import { defineConfig } from "react-doctor/api";

/**
 * React Doctor gate for the web client.
 *
 * `pnpm doctor` is the local full report; `pnpm doctor:ci` is what CI runs and
 * fails only on correctness errors, so pre-existing warnings stay advisory
 * while a new error cannot land.
 */
export default defineConfig({
  ignore: {
    files: [
      // Manual Playwright audit tooling, not shipped application code.
      "audit/**",
      // Built output is scanned through `src` instead.
      "dist/**",
      // ts-rs generated wire bindings; regenerate these, do not edit them.
      "src/api/types/**",
    ],
  },
});
