import { defineConfig } from "vitest/config";

// Standalone config for pure-logic unit tests (no Svelte/DOM), so it stays
// independent of the SvelteKit Vite plugin used for the app build.
export default defineConfig({
    test: {
        include: ["src/**/*.test.ts"],
        environment: "node",
    },
});
