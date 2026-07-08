<script lang="ts">
    import "./onboarding.css";
    import { Button, Input, Select, Toggle, toast } from "anasthasia";
    import AdvancedTiming from "$lib/AdvancedTiming.svelte";
    import InfoTip from "$lib/InfoTip.svelte";
    import { onMountAsync } from "$lib/lifecycle";
    import ModelSetup from "$lib/setup/ModelSetup.svelte";
    import YtDlpSetup from "$lib/setup/YtDlpSetup.svelte";
    import { settingOptions } from "$lib/settings-options";
    import {
        commands,
        events,
        type AppSettings,
        type ModelDownloadEvent,
        type ModelInfo,
    } from "../../types/bindings";

    type CompleteAppSettings = Required<AppSettings>;
    let settings: CompleteAppSettings | null = $state(null);
    let models: ModelInfo[] = $state([]);
    let selectedModel = $state("small");
    let modelDownload: ModelDownloadEvent | null = $state(null);
    let installingModel = $state(false);

    let systemYtDlpFound = $state(false);
    let managedYtDlpInstalled = $state(false);
    let installingYtDlp = $state(false);
    let ytDlpDownload: ModelDownloadEvent | null = $state(null);

    let step = $state(0);
    let wantAdvanced = $state(false);
    let showAdvancedTiming = $state(false);

    let steps = $derived([
        "welcome",
        "model",
        "ytdlp",
        "howto",
        "choice",
        ...(wantAdvanced ? ["transcription", "appearance"] : []),
    ]);
    let current = $derived(steps[step] ?? "welcome");
    let isLast = $derived(step === steps.length - 1);

    let selectedModelInfo = $derived(models.find((model) => model.id === selectedModel) ?? null);
    let hasModel = $derived(models.some((model) => model.installed));

    let ytDlpReady = $derived.by(() =>
        settings?.ytDlpSource === "system" ? systemYtDlpFound : managedYtDlpInstalled,
    );
    onMountAsync(async () => {
        const [snapshot, catalog, toolStatuses, hasSystemYtDlp] = await Promise.all([
            commands.getAppSnapshot(),
            commands.getModelCatalog(),
            commands.getToolStatuses(),
            commands.systemYtDlpAvailable(),
        ]);
        if (snapshot.status === "ok") {
            settings = snapshot.data.settings as CompleteAppSettings;
            selectedModel = settings.model;
        }
        if (catalog.status === "ok") models = catalog.data;
        systemYtDlpFound = hasSystemYtDlp;
        managedYtDlpInstalled = toolStatuses.some(
            (tool) => tool.tool === "yt-dlp" && tool.source === "managed",
        );
        return [
            await events.modelDownloadEvent.listen((event) => {
                if (event.payload.modelId === "yt-dlp") {
                    ytDlpDownload = event.payload;
                    installingYtDlp = !["ready", "failed", "cancelled"].includes(
                        event.payload.state,
                    );
                    if (event.payload.state === "ready") managedYtDlpInstalled = true;
                    if (event.payload.error) toast.danger(event.payload.error, { title: "yt-dlp" });
                    return;
                }
                modelDownload = event.payload;
                installingModel = !["ready", "failed", "cancelled"].includes(event.payload.state);
                if (event.payload.error)
                    toast.danger(event.payload.error, { title: "Model download" });
            }),
            await events.settingsUpdatedEvent.listen((event) => {
                settings = event.payload.settings as CompleteAppSettings;
            }),
        ];
    });

    async function installManagedYtDlp() {
        installingYtDlp = true;
        ytDlpDownload = null;
        const result = await commands.installYtDlp();
        installingYtDlp = false;
        if (result.status === "error") {
            toast.danger(result.error, { title: "yt-dlp" });
            return;
        }
        managedYtDlpInstalled = true;
        toast.success("yt-dlp is ready.");
    }

    async function cancelYtDlpDownload() {
        const result = await commands.cancelToolDownload();
        if (result.status === "error") toast.danger(result.error);
    }

    async function installSelectedModel() {
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
        toast.success(`${result.data.label} is ready.`);
    }

    async function cancelModelDownload() {
        const result = await commands.cancelModelDownload();
        if (result.status === "error") toast.danger(result.error);
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
    }

    function next() {
        if (isLast) void finish();
        else step += 1;
    }

    function back() {
        if (step > 0) step -= 1;
    }

    function chooseAdvanced(yes: boolean) {
        wantAdvanced = yes;
        if (yes) step += 1;
        else void finish();
    }

    async function finish() {
        if (!settings) return;
        const result = await commands.updateSettings({ ...settings, onboarded: true });
        if (result.status === "error") {
            toast.danger(result.error, { title: "Could not finish setup" });
            return;
        }
        await commands.showWindow("main");
        await commands.hideWindow("onboarding");
    }
</script>

<main class="onboarding">
    <div class="onboarding-progress" aria-hidden="true">
        {#each steps as id, index (id)}
            <span class="onboarding-pip" class:done={index < step} class:active={index === step}
            ></span>
        {/each}
    </div>

    <section class="onboarding-body">
        {#if current === "welcome"}
            <div class="onboarding-hero">
                <img class="onboarding-icon" src="/app-icon.png" alt="" aria-hidden="true" />
                <div>
                    <span class="anasthasia-label">KAIGAI 字</span>
                    <h1 class="text-accent-gradient">Welcome to Kaigai</h1>
                </div>
            </div>
            <p class="onboarding-lead">
                Live, on-device subtitles for Japanese (and other) streams. Audio never leaves your
                device — transcription and translation run locally with Whisper.
            </p>
            <p class="onboarding-lead">
                This quick setup downloads a model and shows you how it works.
            </p>
        {:else if current === "model"}
            <h1>Choose a speech model</h1>
            <p class="onboarding-lead">
                Bigger models are more accurate but slower and larger. <strong>Small</strong> is a good
                starting point. It downloads once and then runs fully offline.
            </p>
            <div class="form-stack">
                <ModelSetup
                    {models}
                    bind:selectedModel
                    {modelDownload}
                    {installingModel}
                    {installSelectedModel}
                    {cancelModelDownload}
                    {setCoreMlEnabled}
                />
                {#if !hasModel}
                    <p class="onboarding-note">A model is required before subtitles can run.</p>
                {/if}
            </div>
        {:else if current === "ytdlp" && settings}
            <h1>Stream resolution tool</h1>
            <p class="onboarding-lead">
                Kaigai uses <strong>yt-dlp</strong> to resolve stream URLs. It changes often as streaming
                sites change, so either point Kaigai at a copy you already keep updated yourself, or let
                it manage and update its own copy automatically.
            </p>
            <div class="form-stack">
                <YtDlpSetup
                    bind:settings
                    {ytDlpDownload}
                    {installingYtDlp}
                    {systemYtDlpFound}
                    {managedYtDlpInstalled}
                    choiceLayout
                    showReadyNote
                    {installManagedYtDlp}
                    {cancelYtDlpDownload}
                />
            </div>
        {:else if current === "howto"}
            <h1>How it works</h1>
            <ul class="onboarding-steps">
                <li>
                    <strong>Paste a link</strong> into the floating bar and press
                    <strong>Enter</strong>.
                </li>
                <li>
                    The bar turns into your <strong>subtitle overlay</strong> — drag it over the video,
                    resize from a corner.
                </li>
                <li>
                    Open <strong>Settings</strong> or this tour again anytime from the
                    <strong>menu-bar icon</strong>.
                </li>
                <li>
                    <strong>Member-only or age-restricted</strong> stream? Add your browser sign-in
                    under
                    <strong>Settings → Stream access</strong>.
                </li>
                <li>
                    Press <strong>Esc</strong> or use the tray to <strong>quit</strong> (that also stops
                    subtitles).
                </li>
            </ul>
        {:else if current === "choice"}
            <h1>Set up advanced options?</h1>
            <p class="onboarding-lead">
                The defaults work well for most streams. You can fine-tune language, output, and
                overlay appearance now, or skip and change them later in Settings.
            </p>
        {:else if current === "transcription" && settings}
            <h1>Transcription</h1>
            <div class="form-stack">
                <div class="field">
                    <div class="field-head">
                        <span class="anasthasia-label">Source language</span>
                        <InfoTip
                            text="The language spoken in the stream. Choose Auto-detect when it may vary."
                        />
                    </div>
                    <Select
                        options={settingOptions.language}
                        bind:value={settings.sourceLanguage}
                    />
                </div>
                <div class="field">
                    <div class="field-head">
                        <span class="anasthasia-label">Subtitle output</span>
                        <InfoTip
                            text="Translate to English produces English subtitles. Transcribe keeps the spoken language."
                        />
                    </div>
                    <Select options={settingOptions.task} bind:value={settings.task} />
                </div>
                <div class="field">
                    <div class="field-head">
                        <span class="anasthasia-label">Line cutting</span>
                        <InfoTip
                            text="Adaptive listens for natural pauses so lines break at sentences; Fixed cuts on a steady timer."
                        />
                    </div>
                    <Select options={settingOptions.chunk} bind:value={settings.chunkMode} />
                </div>
                <div class="field">
                    <div class="field-head">
                        <span class="anasthasia-label">Speech sensitivity</span>
                        <InfoTip
                            text="Silero detects human speech before Whisper runs. High keeps quieter voices; Strict rejects more music and background noise."
                        />
                    </div>
                    <Select
                        options={settingOptions.vadSensitivity}
                        bind:value={settings.vadSensitivity}
                    />
                </div>

                <button
                    type="button"
                    class="disclosure"
                    onclick={() => (showAdvancedTiming = !showAdvancedTiming)}
                >
                    {showAdvancedTiming ? "Hide" : "Show"} advanced timing
                </button>
                {#if showAdvancedTiming}
                    <AdvancedTiming bind:settings />
                {/if}
            </div>
        {:else if current === "appearance" && settings}
            <h1>Overlay appearance</h1>
            <div class="form-stack">
                <Select
                    options={settingOptions.fontFamily}
                    label="Font"
                    bind:value={settings.fontFamily}
                />
                <div class="action-row">
                    <Input
                        label="Size (px)"
                        type="number"
                        min="16"
                        max="96"
                        bind:value={settings.fontSizePx}
                    />
                    <Input
                        label="Weight"
                        type="number"
                        min="100"
                        max="900"
                        step="100"
                        bind:value={settings.fontWeight}
                    />
                </div>
                <div class="action-row">
                    <label class="native-color-field">
                        <span class="anasthasia-label">Text</span>
                        <input type="color" bind:value={settings.textColor} />
                    </label>
                    <label class="native-color-field">
                        <span class="anasthasia-label">Background</span>
                        <input type="color" bind:value={settings.backgroundColor} />
                    </label>
                </div>
                <label class="slider-field">
                    <span class="anasthasia-label"
                        >Background opacity <em>{Math.round(settings.backgroundOpacity * 100)}%</em
                        ></span
                    >
                    <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.05"
                        bind:value={settings.backgroundOpacity}
                    />
                </label>
                <Toggle label="Keep overlay always on top" bind:checked={settings.alwaysOnTop} />
            </div>
        {/if}
    </section>

    <footer class="onboarding-footer">
        {#if step > 0}
            <Button variant="ghost" onclick={back}>Back</Button>
        {:else}
            <span></span>
        {/if}

        {#if current === "choice"}
            <div class="action-row">
                <Button variant="secondary" onclick={() => chooseAdvanced(false)}
                    >Use defaults</Button
                >
                <Button variant="primary" onclick={() => chooseAdvanced(true)}>Customize</Button>
            </div>
        {:else}
            <Button
                variant="primary"
                onclick={next}
                disabled={(current === "model" && !hasModel) ||
                    (current === "ytdlp" && !ytDlpReady)}
            >
                {isLast ? "Finish" : "Continue"}
            </Button>
        {/if}
    </footer>
</main>
