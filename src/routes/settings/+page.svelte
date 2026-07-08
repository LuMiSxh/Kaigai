<script lang="ts">
    import "./settings.css";

    import { tick } from "svelte";
    import { Button, Panel, toast } from "anasthasia";
    import AccessSettings from "$lib/settings/AccessSettings.svelte";
    import AppearanceSettings from "$lib/settings/AppearanceSettings.svelte";
    import EngineSettings from "$lib/settings/EngineSettings.svelte";
    import ToolSettings from "$lib/settings/ToolSettings.svelte";
    import AppHeader from "$lib/AppHeader.svelte";
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
    let selectedModel: string = $state("small");
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

    onMountAsync(async () => {
        const [snapshot, catalog] = await Promise.all([
            commands.getAppSnapshot(),
            commands.getModelCatalog(),
        ]);
        if (snapshot.status === "error") {
            toast.danger(snapshot.error);
            return [];
        }
        syncSettings(snapshot.data.settings);
        if (catalog.status === "ok") models = catalog.data;
        if (DEV_BUILD) toolStatuses = await commands.getToolStatuses();
        return [
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
            await events.settingsUpdatedEvent.listen(async (event) => {
                syncSettings(event.payload.settings);
                const refreshed = await commands.getModelCatalog();
                if (refreshed.status === "ok") models = refreshed.data;
            }),
        ];
    });

    async function saveSettings() {
        if (!settings) return;
        saving = true;
        const result = await commands.updateSettings(settings);
        saving = false;
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
        checkingYtDlpUpdate = true;
        const result = await commands.checkYtDlpUpdate();
        checkingYtDlpUpdate = false;
        if (result.status === "error") {
            toast.danger(result.error, { title: "yt-dlp" });
            return;
        }
        ytDlpUpdateAvailable = result.data;
        toast.info(result.data ? `yt-dlp ${result.data} is available.` : "yt-dlp is up to date.");
    }

    async function installYtDlpUpdate() {
        installingYtDlp = true;
        ytDlpDownload = null;
        const result = await commands.installYtDlp();
        installingYtDlp = false;
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
        resetting = true;
        const result = await commands.resetApp();
        resetting = false;
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
        installingModel = true;
        modelDownload = null;
        const result = await commands.installModel(selectedModel);
        installingModel = false;
        if (result.status === "error") {
            toast.danger(result.error, { title: "Model download" });
            return;
        }
        const catalog = await commands.getModelCatalog();
        if (catalog.status === "ok") models = catalog.data;
        toast.success(`${result.data.label} is ready — it will be used for the next session.`);
    }

    async function setCoreMlEnabled(enabled: boolean) {
        if (!selectedModelInfo) return;
        installingModel = true;
        modelDownload = null;
        const result = await commands.setCoreMlEnabled(selectedModelInfo.id, enabled);
        installingModel = false;
        if (result.status === "error") {
            toast.danger(result.error, { title: "Core ML" });
            return;
        }
        const catalog = await commands.getModelCatalog();
        if (catalog.status === "ok") models = catalog.data;
        toast.success(
            enabled
                ? `Neural Engine enabled for ${result.data.label}.`
                : `Neural Engine data removed for ${result.data.label}.`,
        );
    }

    async function uninstallSelectedModel() {
        if (!selectedModelInfo) return;
        removingModel = true;
        const wasActive = selectedModelInfo.active;
        const result = await commands.uninstallModel(selectedModelInfo.id);
        removingModel = false;
        confirmingModelRemoval = null;
        if (result.status === "error") {
            toast.danger(result.error, { title: "Could not remove model" });
            return;
        }
        const catalog = await commands.getModelCatalog();
        if (catalog.status === "ok") models = catalog.data;
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
                            <p class="panel-note" style="color: var(--color-danger, #d23c40)">
                                This will delete all downloaded models and managed yt-dlp, reset all
                                settings, and restart the setup tour. This cannot be undone.
                            </p>
                            <div
                                class="action-row"
                                style:margin-top="0.75rem"
                                bind:this={resetActions}
                            >
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
