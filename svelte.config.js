import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
    preprocess: vitePreprocess(),
    kit: {
        adapter: adapter({
            // Keep the fallback separate from the prerendered Tauri window routes.
            fallback: "200.html",
        }),
    },
};
