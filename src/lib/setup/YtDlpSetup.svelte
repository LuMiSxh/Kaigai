<script lang="ts">
    import { Badge, Button, Card, Select } from "anasthasia";
    import DownloadProgress from "$lib/DownloadProgress.svelte";
    import InfoTip from "$lib/InfoTip.svelte";
    import { settingOptions } from "$lib/settings-options";
    import type { AppSettings, ModelDownloadEvent } from "../../types/bindings";

    type CompleteAppSettings = Required<AppSettings>;

    let {
        settings = $bindable(),
        ytDlpDownload,
        installingYtDlp,
        systemYtDlpFound = false,
        managedYtDlpInstalled = false,
        ytDlpUpdateAvailable = null,
        checkingYtDlpUpdate = false,
        choiceLayout = false,
        showReadyNote = false,
        installManagedYtDlp,
        checkForYtDlpUpdate,
        installYtDlpUpdate,
        cancelYtDlpDownload,
    }: {
        settings: CompleteAppSettings;
        ytDlpDownload: ModelDownloadEvent | null;
        installingYtDlp: boolean;
        systemYtDlpFound?: boolean;
        managedYtDlpInstalled?: boolean;
        ytDlpUpdateAvailable?: string | null;
        checkingYtDlpUpdate?: boolean;
        choiceLayout?: boolean;
        showReadyNote?: boolean;
        installManagedYtDlp?: () => Promise<void>;
        checkForYtDlpUpdate?: () => Promise<void>;
        installYtDlpUpdate?: () => Promise<void>;
        cancelYtDlpDownload: () => Promise<void>;
    } = $props();

    let ytDlpDownloadActive = $derived.by(
        () =>
            ytDlpDownload !== null &&
            ["queued", "downloading", "verifying", "installing"].includes(ytDlpDownload.state),
    );
    let ytDlpReady = $derived.by(() =>
        settings.ytDlpSource === "system" ? systemYtDlpFound : managedYtDlpInstalled,
    );
</script>

{#if choiceLayout}
    <div class="action-row">
        <Button
            variant={settings.ytDlpSource === "system" ? "primary" : "secondary"}
            onclick={() => (settings.ytDlpSource = "system")}
        >
            Use system yt-dlp
        </Button>
        <Button
            variant={settings.ytDlpSource === "managed" ? "primary" : "secondary"}
            onclick={() => (settings.ytDlpSource = "managed")}
        >
            Let Kaigai manage it
        </Button>
    </div>
{:else}
    <div class="settings-row">
        <div class="settings-row-label">
            <span class="anasthasia-label">yt-dlp source</span>
            <InfoTip
                text="Use your own system install if you already keep one updated, or let Kaigai download and update its own copy."
            />
        </div>
        <Select options={settingOptions.ytDlpSource} bind:value={settings.ytDlpSource} />
    </div>
{/if}

{#if settings.ytDlpSource === "system" && choiceLayout}
    <Card>
        <div class="model-summary">
            <div>
                <span class="anasthasia-label">System yt-dlp</span>
                <p class="model-summary-meta">
                    {systemYtDlpFound ? "Found on PATH" : "Not found on PATH"}
                </p>
            </div>
            <Badge variant={systemYtDlpFound ? "success" : "danger"}>
                {systemYtDlpFound ? "found" : "missing"}
            </Badge>
        </div>
    </Card>
    {#if !systemYtDlpFound}
        <p class="onboarding-note">
            Install yt-dlp and make sure it's on your PATH, or switch to "Let Kaigai manage it"
            above.
        </p>
    {/if}
{:else if settings.ytDlpSource === "managed"}
    {#if choiceLayout}
        <Card>
            <div class="model-summary">
                <div>
                    <span class="anasthasia-label">Managed yt-dlp</span>
                    <p class="model-summary-meta">
                        {managedYtDlpInstalled
                            ? "Installed, Kaigai keeps it updated"
                            : "Not installed yet"}
                    </p>
                </div>
                <Badge variant={managedYtDlpInstalled ? "success" : "mono"}>
                    {managedYtDlpInstalled ? "installed" : "download required"}
                </Badge>
            </div>
        </Card>
    {:else}
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Manual update</span>
            </div>
            <div class="action-row">
                <Button
                    variant="ghost"
                    size="sm"
                    onclick={checkForYtDlpUpdate}
                    loading={checkingYtDlpUpdate}
                    loadingLabel="Checking"
                    disabled={!checkForYtDlpUpdate}
                >
                    Check for update
                </Button>
                {#if ytDlpUpdateAvailable}
                    <Button
                        variant="primary"
                        size="sm"
                        onclick={installYtDlpUpdate}
                        loading={installingYtDlp}
                        loadingLabel="Updating"
                        disabled={!installYtDlpUpdate}
                    >
                        Update to {ytDlpUpdateAvailable}
                    </Button>
                {/if}
                {#if installingYtDlp}
                    <Button variant="ghost" size="sm" onclick={cancelYtDlpDownload}>Cancel</Button>
                {/if}
            </div>
        </div>
    {/if}

    {#if ytDlpDownloadActive && ytDlpDownload}
        <DownloadProgress download={ytDlpDownload} />
    {/if}

    {#if choiceLayout}
        <div class="action-row">
            <Button
                variant="primary"
                onclick={installManagedYtDlp}
                disabled={managedYtDlpInstalled || installingYtDlp || !installManagedYtDlp}
                loading={installingYtDlp}
                loadingLabel="Preparing yt-dlp"
            >
                {managedYtDlpInstalled ? "Installed" : "Download and use"}
            </Button>
            {#if installingYtDlp}
                <Button variant="ghost" onclick={cancelYtDlpDownload}>Cancel</Button>
            {/if}
        </div>
    {/if}
{/if}

{#if showReadyNote && !ytDlpReady}
    <p class="onboarding-note">yt-dlp is required before subtitles can run.</p>
{/if}
