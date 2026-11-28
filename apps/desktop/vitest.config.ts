import { svelte, vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    tailwindcss(),
    svelte({
      configFile: false,
      preprocess: vitePreprocess(),
    }),
  ],
  resolve: { conditions: ["browser"] },
  test   : {
    environment: "happy-dom",
    coverage   : {
      include   : ["src/**/*.{svelte,ts}"],
      thresholds: {
        branches  : 80,
        functions : 80,
        lines     : 80,
        statements: 80,
      },
    },
  },
});
