<script lang="ts">
    import type { Snippet } from "svelte";
    import type { SessionStatus } from "../types/bindings";
    import StatusPill from "./StatusPill.svelte";

    let {
        kicker,
        title,
        state,
        actions,
    }: {
        kicker: string;
        title: string;
        state?: SessionStatus;
        actions?: Snippet;
    } = $props();
</script>

<header class="app-bar">
    <div class="app-bar-brand">
        <img class="brand-mark" src="/app-icon.png" alt="" aria-hidden="true" />
        <div class="app-bar-copy">
            <span class="anasthasia-label">{kicker}</span>
            <h1 class="text-accent-gradient">{title}</h1>
        </div>
    </div>
    <div class="app-bar-tools">
        {#if state}
            <StatusPill {state} />
        {/if}
        {@render actions?.()}
    </div>
</header>
