<script lang="ts">
    import { Alert, Panel, Select } from "anasthasia";
    import AdvancedTiming from "$lib/AdvancedTiming.svelte";
    import InfoTip from "$lib/InfoTip.svelte";
    import ModelSetup from "$lib/setup/ModelSetup.svelte";
    import { settingOptions } from "$lib/settings-options";
    import type { AppSettings, ModelDownloadEvent, ModelInfo } from "../../types/bindings";

    type CompleteAppSettings = Required<AppSettings>;

    let {
        settings = $bindable(),
        models,
        selectedModel = $bindable(),
        modelDownload,
        installingModel,
        removingModel,
        confirmingModelRemoval = $bindable(),
        showAdvancedTiming = $bindable(),
        installSelectedModel,
        cancelModelDownload,
        setCoreMlEnabled,
        uninstallSelectedModel,
    }: {
        settings: CompleteAppSettings;
        models: ModelInfo[];
        selectedModel: string;
        modelDownload: ModelDownloadEvent | null;
        installingModel: boolean;
        removingModel: boolean;
        confirmingModelRemoval: string | null;
        showAdvancedTiming: boolean;
        installSelectedModel: () => Promise<void>;
        cancelModelDownload: () => Promise<void>;
        setCoreMlEnabled: (enabled: boolean) => Promise<void>;
        uninstallSelectedModel: () => Promise<void>;
    } = $props();

    let activeModelInfo = $derived(models.find((model) => model.id === settings.model) ?? null);
</script>

<Panel title="Model">
    <ModelSetup
        {models}
        bind:selectedModel
        {modelDownload}
        {installingModel}
        {removingModel}
        bind:confirmingModelRemoval
        showActiveWarning
        showProvider
        showDescription
        allowRemoval
        {installSelectedModel}
        {cancelModelDownload}
        {setCoreMlEnabled}
        {uninstallSelectedModel}
    />
</Panel>

<Panel title="Transcription">
    <div class="settings-rows">
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Source language</span>
                <InfoTip
                    text="The language spoken in the stream. Choose Auto-detect when the source language may vary."
                />
            </div>
            <Select options={settingOptions.language} bind:value={settings.sourceLanguage} />
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Subtitle output</span>
                <InfoTip
                    text="Translate to English produces English subtitles. Transcribe keeps the subtitles in the spoken language."
                />
            </div>
            <div class="form-stack">
                <Select options={settingOptions.task} bind:value={settings.task} />
                {#if settings.task === "translate" && activeModelInfo && !activeModelInfo.supportsTranslate}
                    <Alert variant="warning">
                        <strong>{activeModelInfo.label}</strong> is a distilled model and does not support
                        translation — subtitles will stay in the source language. Switch to Medium, Large
                        v3, or another non-distilled model.
                    </Alert>
                {/if}
            </div>
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Caption mode</span>
                <InfoTip
                    text="Stable waits for final utterance boundaries and keeps the last good caption during short pauses. Live shows rolling drafts sooner, with more churn."
                />
            </div>
            <Select options={settingOptions.captionMode} bind:value={settings.captionMode} />
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Line cutting</span>
                <InfoTip
                    text="Adaptive listens for natural pauses so lines break at sentences; Fixed cuts on a steady timer."
                />
            </div>
            <Select options={settingOptions.chunk} bind:value={settings.chunkMode} />
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Speech sensitivity</span>
                <InfoTip
                    text="Silero detects human speech before Whisper runs. High keeps quieter voices; Strict rejects more music and background noise."
                />
            </div>
            <Select options={settingOptions.vadSensitivity} bind:value={settings.vadSensitivity} />
        </div>
    </div>

    <button
        type="button"
        class="disclosure section-spaced"
        onclick={() => (showAdvancedTiming = !showAdvancedTiming)}
    >
        {showAdvancedTiming ? "Hide" : "Show"} advanced timing
    </button>
    {#if showAdvancedTiming}
        <div class="section-spaced">
            <AdvancedTiming bind:settings />
        </div>
    {/if}
</Panel>
