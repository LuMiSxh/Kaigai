<script lang="ts">
    import { Tooltip } from "anasthasia";

    let { text }: { text: string } = $props();
</script>

<Tooltip content={text} placement="top" portal>
    <button type="button" class="info-tip-trigger" aria-label={`More information: ${text}`}>
        <span aria-hidden="true">i</span>
    </button>
</Tooltip>
