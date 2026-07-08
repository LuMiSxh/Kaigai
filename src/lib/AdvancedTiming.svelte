<script lang="ts">
    import { FieldRow, Input } from "anasthasia";
    import type { AppSettings } from "../types/bindings";

    let { settings = $bindable() }: { settings: Required<AppSettings> } = $props();
</script>

<div class="form-stack">
    <FieldRow label="Cut timing" hint="Hard limit per line and silence required to finalize it.">
        <Input
            label="Maximum (ms)"
            type="number"
            min="4000"
            max="30000"
            step="500"
            bind:value={settings.maximumChunkMs}
        />
        <Input
            label="End silence (ms)"
            type="number"
            min="150"
            max="2000"
            step="50"
            bind:value={settings.endSilenceMs}
        />
    </FieldRow>
    <FieldRow
        label="Streaming"
        hint="Audio before the first preview and context retained across line boundaries."
    >
        <Input
            label="First preview (ms)"
            type="number"
            min="500"
            max="20000"
            step="500"
            bind:value={settings.minimumChunkMs}
        />
        <Input
            label="Overlap (ms)"
            type="number"
            min="0"
            max="3000"
            step="100"
            bind:value={settings.overlapMs}
        />
    </FieldRow>
</div>
