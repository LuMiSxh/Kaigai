<script lang="ts">
    import "./settings.css";

    import { tick } from "svelte";
    import { Button, Panel, toast } from "anasthasia";
    import AccessSettings from "$lib/settings/AccessSettings.svelte";
    import AppearanceSettings from "$lib/settings/AppearanceSettings.svelte";
    import EngineSettings from "$lib/settings/EngineSettings.svelte";
    import ToolSettings from "$lib/settings/ToolSettings.svelte";
    import AppHeader from "$lib/AppHeader.svelte";
    import { whileBusy } from "$lib/async";
    import { onMountAsync } from "$lib/lifecycle";
    import {
        commands,
        events,
        type AppSettings,
        type ModelDownloadEvent,
        type ModelInfo,
        type ToolStatus,
    } from "../../types/bindings";

    type CompleteAppSettings = Required<AppSettings>;

    const DEV_BUILD = import.meta.env.DEV;

    let settings: CompleteAppSettings | null = $state(null);
    let saving: boolean = $state(false);
    let toolStatuses: ToolStatus[] = $state([]);
    let models: ModelInfo[] = $state([]);
    let selectedModel: string = $state("medium");
    let modelDownload: ModelDownloadEvent | null = $state(null);
    let installingModel = $state(false);
    let removingModel = $state(false);
    let confirmingModelRemoval: string | null = $state(null);
    let showAdvancedTiming = $state(false);

    let checkingYtDlpUpdate = $state(false);
    let installingYtDlp = $state(false);
    let ytDlpUpdateAvailable: string | null = $state(null);
    let ytDlpDownload: ModelDownloadEvent | null = $state(null);

    let confirmingReset = $state(false);
    let resetting = $state(false);
    let activeTab = $state<"engine" | "appearance" | "access" | "maintenance">("engine");
    let resetActions: HTMLDivElement | undefined = $state();

    let selectedModelInfo = $derived(models.find((model) => model.id === selectedModel) ?? null);

    function syncSettings(next: AppSettings) {
        settings = next as CompleteAppSettings;
        selectedModel = settings.model;
    }

    onMountAsync(async (onCleanup) => {
        const [snapshot, catalog] = await Promise.all([
            commands.getAppSnapshot(),
            commands.getModelCatalog(),
        ]);
        if (snapshot.status === "error") {
            toast.danger(snapshot.error);
            return;
        }
        syncSettings(snapshot.data.settings);
        if (catalog.status === "ok") {
            models = catalog.data;
        } else {
            toast.danger(catalog.error, { title: "Could not load models" });
        }
        toolStatuses = await commands.getToolStatuses();
        onCleanup(
            await events.modelDownloadEvent.listen((event) => {
                if (event.payload.modelId === "yt-dlp") {
                    ytDlpDownload = event.payload;
                    installingYtDlp = !["ready", "failed", "cancelled"].includes(
                        event.payload.state,
                    );
                    if (event.payload.state === "ready") {
                        ytDlpUpdateAvailable = null;
                        void commands.getToolStatuses().then((statuses) => {
                            toolStatuses = statuses;
                        });
                    }
                    if (event.payload.error) toast.danger(event.payload.error, { title: "yt-dlp" });
                    return;
                }
                modelDownload = event.payload;
                installingModel = !["ready", "failed", "cancelled"].includes(event.payload.state);
                if (event.payload.error)
                    toast.danger(event.payload.error, { title: "Model download" });
            }),
        );
        onCleanup(
            await events.settingsUpdatedEvent.listen(async (event) => {
                syncSettings(event.payload.settings);
                await refreshModels();
            }),
        );
    });

    async function saveSettings() {
        if (!settings) return;
        const currentSettings = settings;
        const result = await whileBusy(
            (busy) => (saving = busy),
            () => commands.updateSettings(currentSettings),
        );
        if (result.status === "error") {
            toast.danger(result.error, { title: "Could not save" });
            return;
        }
        syncSettings(result.data);
        toast.success("Settings saved.");
    }

    async function refreshTools() {
        toolStatuses = await commands.getToolStatuses();
        toast.info("Tool versions refreshed.");
    }

    async function checkForYtDlpUpdate() {
        const result = await whileBusy(
            (busy) => (checkingYtDlpUpdate = busy),
            commands.checkYtDlpUpdate,
        );
        if (result.status === "error") {
            toast.danger(result.error, { title: "yt-dlp" });
            return;
        }
        ytDlpUpdateAvailable = result.data;
        toast.info(result.data ? `yt-dlp ${result.data} is available.` : "yt-dlp is up to date.");
    }

    async function installYtDlpUpdate() {
        ytDlpDownload = null;
        const result = await whileBusy((busy) => (installingYtDlp = busy), commands.installYtDlp);
        if (result.status === "error") {
            toast.danger(result.error, { title: "yt-dlp" });
            return;
        }
        ytDlpUpdateAvailable = null;
        toast.success("yt-dlp is up to date.");
    }

    async function cancelYtDlpDownload() {
        const result = await commands.cancelToolDownload();
        if (result.status === "error") toast.danger(result.error);
    }

    async function resetApp() {
        const result = await whileBusy((busy) => (resetting = busy), commands.resetApp);
        if (result.status === "error") {
            toast.danger(result.error, { title: "Reset failed" });
            confirmingReset = false;
        }
        // On success the backend opens onboarding and hides settings — no
        // further action needed here.
    }

    async function confirmResetIntent() {
        confirmingReset = true;
        await tick();
        const cancelButton = resetActions?.querySelector<HTMLButtonElement>("[data-reset-cancel]");
        cancelButton?.scrollIntoView({ block: "center", behavior: "smooth" });
        cancelButton?.focus({ preventScroll: true });
    }

    async function replayTour() {
        if (!settings) return;
        const result = await commands.updateSettings({ ...settings, onboarded: false });
        if (result.status === "error") {
            toast.danger(result.error);
            return;
        }
        syncSettings(result.data);
        await commands.showWindow("onboarding");
    }

    async function installSelectedModel() {
        if (!selectedModelInfo) return;
        modelDownload = null;
        const result = await whileBusy(
            (busy) => (installingModel = busy),
            () => commands.installModel(selectedModel),
        );
        if (result.status === "error") {
            toast.danger(result.error, { title: "Model download" });
            return;
        }
        await refreshModels();
        toast.success(`${result.data.label} is ready — it will be used for the next session.`);
    }

    async function setCoreMlEnabled(enabled: boolean) {
        if (!selectedModelInfo) return;
        modelDownload = null;
        const result = await whileBusy(
            (busy) => (installingModel = busy),
            () => commands.setCoreMlEnabled(selectedModelInfo.id, enabled),
        );
        if (result.status === "error") {
            toast.danger(result.error, { title: "Core ML" });
            return;
        }
        await refreshModels();
        toast.success(
            enabled
                ? `Neural Engine enabled for ${result.data.label}.`
                : `Neural Engine data removed for ${result.data.label}.`,
        );
    }

    async function uninstallSelectedModel() {
        if (!selectedModelInfo) return;
        const wasActive = selectedModelInfo.active;
        const result = await whileBusy(
            (busy) => (removingModel = busy),
            () => commands.uninstallModel(selectedModelInfo.id),
        );
        confirmingModelRemoval = null;
        if (result.status === "error") {
            toast.danger(result.error, { title: "Could not remove model" });
            return;
        }
        await refreshModels();
        toast.success(
            wasActive
                ? `${selectedModelInfo.label} was removed. Choose another model before starting subtitles.`
                : `${selectedModelInfo.label} was removed.`,
        );
    }

    async function cancelModelDownload() {
        const result = await commands.cancelModelDownload();
        if (result.status === "error") toast.danger(result.error);
    }

    async function refreshModels() {
        const result = await commands.getModelCatalog();
        if (result.status === "ok") {
            models = result.data;
        } else {
            toast.danger(result.error, { title: "Could not refresh models" });
        }
    }
</script>

<main class="app-shell settings-shell">
    <AppHeader kicker="Configuration" title="Settings">
        {#snippet actions()}
            <Button variant="primary" onclick={saveSettings} loading={saving} loadingLabel="Saving"
                >Save</Button
            >
        {/snippet}
    </AppHeader>

    {#if settings}
        <div class="settings-layout">
            <nav class="settings-nav" aria-label="Settings sections">
                <button
                    class="settings-nav-btn"
                    class:is-active={activeTab === "engine"}
                    onclick={() => (activeTab = "engine")}>Engine</button
                >
                <button
                    class="settings-nav-btn"
                    class:is-active={activeTab === "appearance"}
                    onclick={() => (activeTab = "appearance")}>Appearance</button
                >
                <button
                    class="settings-nav-btn"
                    class:is-active={activeTab === "access"}
                    onclick={() => (activeTab = "access")}>Access</button
                >
                <button
                    class="settings-nav-btn"
                    class:is-active={activeTab === "maintenance"}
                    onclick={() => (activeTab = "maintenance")}>Maintenance</button
                >
            </nav>

            <div class="settings-body">
                {#if activeTab === "engine"}
                    <EngineSettings
                        bind:settings
                        {models}
                        bind:selectedModel
                        {modelDownload}
                        {installingModel}
                        {removingModel}
                        bind:confirmingModelRemoval
                        bind:showAdvancedTiming
                        {installSelectedModel}
                        {cancelModelDownload}
                        {setCoreMlEnabled}
                        {uninstallSelectedModel}
                    />
                {:else if activeTab === "appearance"}
                    <AppearanceSettings bind:settings />
                {:else if activeTab === "access"}
                    <AccessSettings bind:settings />
                {:else if activeTab === "maintenance"}
                    <ToolSettings
                        bind:settings
                        {toolStatuses}
                        {ytDlpDownload}
                        {ytDlpUpdateAvailable}
                        {checkingYtDlpUpdate}
                        {installingYtDlp}
                        devBuild={DEV_BUILD}
                        {replayTour}
                        {refreshTools}
                        {checkForYtDlpUpdate}
                        {installYtDlpUpdate}
                        {cancelYtDlpDownload}
                    />

                    <Panel title="Danger zone">
                        {#if confirmingReset}
                            <p class="panel-note danger-note">
                                This will delete all downloaded models and managed yt-dlp, reset all
                                settings, and restart the setup tour. This cannot be undone.
                            </p>
                            <div class="action-row section-spaced" bind:this={resetActions}>
                                <Button
                                    variant="danger"
                                    size="sm"
                                    onclick={resetApp}
                                    loading={resetting}
                                    loadingLabel="Resetting"
                                >
                                    Yes, reset everything
                                </Button>
                                <Button
                                    variant="ghost"
                                    size="sm"
                                    data-reset-cancel
                                    onclick={() => (confirmingReset = false)}
                                >
                                    Cancel
                                </Button>
                            </div>
                        {:else}
                            <Button variant="danger" size="sm" onclick={confirmResetIntent}>
                                Reset Kaigai…
                            </Button>
                        {/if}
                    </Panel>
                {/if}
            </div>
        </div>
    {/if}
</main>
