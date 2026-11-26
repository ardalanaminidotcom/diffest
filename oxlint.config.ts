import { defineConfig } from "oxlint";
import { config } from "oxlint-config-noir";

export default defineConfig({
  extends  : [config.recommended],
  overrides: [
    {
      files: ["**/*.{svelte,svelte.js,svelte.ts}"],
      env  : { svelte: true },
    },
  ],
});
