<script lang="ts">
    import { ProgressBar } from "anasthasia";
    import { formatBytes, formatEta, formatRate } from "$lib/format";
    import type { ModelDownloadEvent } from "../types/bindings";

    let { download }: { download: ModelDownloadEvent } = $props();
    let progress = $derived(
        download.totalBytes > 0 ? download.downloadedBytes / download.totalBytes : 0,
    );

    function statusText() {
        if (download.state === "downloading") {
            return `${formatRate(download.bytesPerSecond)} · ${formatEta(download.etaSeconds)}`;
        }
        if (download.state === "verifying") return "Verifying checksum…";
        return "Installing…";
    }
</script>

<div class="model-progress">
    <div class="model-progress-copy">
        <strong>{download.state}</strong>
        <span>{formatBytes(download.downloadedBytes)} / {formatBytes(download.totalBytes)}</span>
    </div>
    <ProgressBar value={progress} label={`${download.modelId} download progress`} />
    <small>{statusText()}</small>
</div>
