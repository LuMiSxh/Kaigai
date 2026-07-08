<script lang="ts">
    import { Badge, Button, Card, Panel, PathDisplay, Toggle } from "anasthasia";
    import InfoTip from "$lib/InfoTip.svelte";
    import YtDlpSetup from "$lib/setup/YtDlpSetup.svelte";
    import type { AppSettings, ModelDownloadEvent, ToolStatus } from "../../types/bindings";

    type CompleteAppSettings = Required<AppSettings>;

    let {
        settings = $bindable(),
        toolStatuses,
        ytDlpDownload,
        ytDlpUpdateAvailable,
        checkingYtDlpUpdate,
        installingYtDlp,
        devBuild,
        replayTour,
        refreshTools,
        checkForYtDlpUpdate,
        installYtDlpUpdate,
        cancelYtDlpDownload,
    }: {
        settings: CompleteAppSettings;
        toolStatuses: ToolStatus[];
        ytDlpDownload: ModelDownloadEvent | null;
        ytDlpUpdateAvailable: string | null;
        checkingYtDlpUpdate: boolean;
        installingYtDlp: boolean;
        devBuild: boolean;
        replayTour: () => Promise<void>;
        refreshTools: () => Promise<void>;
        checkForYtDlpUpdate: () => Promise<void>;
        installYtDlpUpdate: () => Promise<void>;
        cancelYtDlpDownload: () => Promise<void>;
    } = $props();
</script>

<Panel title="Updates">
    <div class="settings-rows">
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">yt-dlp updates</span>
                <InfoTip
                    text="yt-dlp breaks often as streaming sites change. When managed, Kaigai can check for and install newer builds on startup."
                />
            </div>
            <Toggle
                label="Check automatically"
                bind:checked={settings.automaticToolUpdates}
                disabled={settings.ytDlpSource !== "managed"}
            />
        </div>
        {#if devBuild}
            <div class="settings-row">
                <div class="settings-row-label">
                    <span class="anasthasia-label">Setup tour</span>
                </div>
                <Button variant="ghost" size="sm" onclick={replayTour}>Replay</Button>
            </div>
        {/if}
    </div>
</Panel>

<Panel title="Stream resolver">
    {#snippet actions()}
        <Button variant="ghost" size="sm" onclick={refreshTools}>Refresh</Button>
    {/snippet}
    <div class="settings-rows">
        <YtDlpSetup
            bind:settings
            {ytDlpDownload}
            {installingYtDlp}
            {ytDlpUpdateAvailable}
            {checkingYtDlpUpdate}
            {checkForYtDlpUpdate}
            {installYtDlpUpdate}
            {cancelYtDlpDownload}
        />
    </div>
</Panel>

{#if devBuild}
    <Panel title="Sidecar tools">
        <div class="form-stack">
            {#each toolStatuses as tool (tool.tool)}
                <Card>
                    <div class="tool-status">
                        <div class="tool-status-copy">
                            <span class="anasthasia-label">{tool.tool}</span>
                            <p class="tool-version">
                                {tool.version ?? "Not available"}
                            </p>
                            <PathDisplay value={tool.path} empty="No path resolved" />
                        </div>
                        <Badge variant={tool.source === "missing" ? "danger" : "mono"}
                            >{tool.source}</Badge
                        >
                    </div>
                </Card>
            {/each}
            <small class="panel-note">
                ffmpeg ships with the app, pinned to a build hash checked at compile time. Managed
                yt-dlp is verified against yt-dlp's own release checksums before install.
            </small>
        </div>
    </Panel>
{/if}
