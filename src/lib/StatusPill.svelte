<script lang="ts">
    import type { SessionStatus } from "../types/bindings";
    import { isBusy, sessionMeta } from "./status";

    let { state }: { state: SessionStatus } = $props();
    let meta = $derived(sessionMeta(state));
</script>

<span
    class="status-pill"
    data-tone={meta.tone}
    class:is-pulsing={isBusy(state) || meta.tone === "live"}
>
    <span class="status-dot"></span>
    {meta.label}
</span>
