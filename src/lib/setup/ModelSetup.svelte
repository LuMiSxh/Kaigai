<script lang="ts">
    import { Alert, Badge, Button, Card, Select, Toggle } from "anasthasia";
    import DownloadProgress from "$lib/DownloadProgress.svelte";
    import InfoTip from "$lib/InfoTip.svelte";
    import { formatBytes } from "$lib/format";
    import type { ModelDownloadEvent, ModelInfo } from "../../types/bindings";

    let {
        models,
        selectedModel = $bindable(),
        modelDownload,
        installingModel,
        removingModel = false,
        confirmingModelRemoval = $bindable(null),
        showActiveWarning = false,
        showProvider = false,
        showDescription = false,
        allowRemoval = false,
        installSelectedModel,
        cancelModelDownload,
        setCoreMlEnabled,
        uninstallSelectedModel,
    }: {
        models: ModelInfo[];
        selectedModel: string;
        modelDownload: ModelDownloadEvent | null;
        installingModel: boolean;
        removingModel?: boolean;
        confirmingModelRemoval?: string | null;
        showActiveWarning?: boolean;
        showProvider?: boolean;
        showDescription?: boolean;
        allowRemoval?: boolean;
        installSelectedModel: () => Promise<void>;
        cancelModelDownload: () => Promise<void>;
        setCoreMlEnabled: (enabled: boolean) => Promise<void>;
        uninstallSelectedModel?: () => Promise<void>;
    } = $props();

    let selectedModelInfo = $derived(models.find((model) => model.id === selectedModel) ?? null);
    let activeModelInfo = $derived(models.find((model) => model.active) ?? null);
    let modelOptions = $derived(
        models.map((model) => ({
            value: model.id,
            label: `${model.label}${showProvider ? ` (${model.provider})` : ""} · ${formatBytes(model.sizeBytes)}${model.installed ? " · installed" : ""}`,
        })),
    );
    let downloadActive = $derived.by(
        () =>
            modelDownload !== null &&
            ["queued", "downloading", "verifying", "installing"].includes(modelDownload.state),
    );

    function modelStatusLabel(model: ModelInfo): string {
        if (model.active) return "active";
        if (model.installed) return "installed";
        return "download required";
    }

    function modelRuntimeLabel(model: ModelInfo): string {
        return model.coreMlEnabled ? "Neural Engine acceleration" : "Mac GPU acceleration";
    }
</script>

<fieldset class="model-fieldset" disabled={installingModel || removingModel}>
    <div class="form-stack">
        {#if showActiveWarning && !activeModelInfo}
            <Alert variant="warning" title="No active model">
                Download or activate a model before starting subtitles.
            </Alert>
        {/if}

        <div class="field">
            <div class="field-head">
                <span class="anasthasia-label">Speech model</span>
                <InfoTip
                    text="The local model that turns speech into text. Bigger models are more accurate but slower and heavier. Downloaded once, then runs fully offline."
                />
            </div>
            <Select options={modelOptions} search bind:value={selectedModel} />
        </div>

        {#if selectedModelInfo}
            <Card>
                <div class="model-summary">
                    <div>
                        <span class="anasthasia-label">{selectedModelInfo.label}</span>
                        <p class="model-summary-meta">
                            {showProvider ? `${selectedModelInfo.provider} · ` : ""}{formatBytes(
                                selectedModelInfo.sizeBytes,
                            )} · {showProvider
                                ? modelRuntimeLabel(selectedModelInfo)
                                : "local model"}
                        </p>
                        {#if showDescription}
                            <p class="model-summary-description">
                                {selectedModelInfo.description}
                            </p>
                        {/if}
                    </div>
                    <Badge
                        variant={selectedModelInfo.active
                            ? "success"
                            : selectedModelInfo.installed
                              ? "accent"
                              : "mono"}
                    >
                        {modelStatusLabel(selectedModelInfo)}
                    </Badge>
                </div>

                {#if selectedModelInfo.coreMlAvailable}
                    <div class="coreml-row">
                        <div class="field-head">
                            <span class="anasthasia-label">Neural Engine</span>
                            <InfoTip
                                text="Uses Core ML to run part of Whisper on Apple Silicon's Neural Engine. It downloads an extra file the first time; turning it off deletes that file."
                            />
                        </div>
                        <Toggle
                            hint={`Recommended on Apple Silicon${selectedModelInfo.coreMlSizeBytes ? ` · +${formatBytes(selectedModelInfo.coreMlSizeBytes)}` : ""}`}
                            checked={selectedModelInfo.coreMlEnabled}
                            disabled={!selectedModelInfo.installed || installingModel}
                            onchange={setCoreMlEnabled}
                        />
                    </div>
                {/if}
            </Card>
        {/if}

        {#if downloadActive && modelDownload}
            <DownloadProgress download={modelDownload} />
        {/if}

        <div class="action-row">
            <Button
                variant="primary"
                onclick={installSelectedModel}
                disabled={!selectedModelInfo || selectedModelInfo.active || installingModel}
                loading={installingModel}
                loadingLabel="Preparing model"
            >
                {selectedModelInfo?.installed ? "Use model" : "Download and use"}
            </Button>
            {#if installingModel}
                <Button variant="ghost" onclick={cancelModelDownload}>Cancel</Button>
            {:else if allowRemoval && selectedModelInfo?.installed}
                {#if confirmingModelRemoval === selectedModelInfo.id}
                    <Button
                        variant="danger"
                        onclick={uninstallSelectedModel}
                        loading={removingModel}
                        loadingLabel="Removing"
                    >
                        Confirm removal
                    </Button>
                    <Button variant="ghost" onclick={() => (confirmingModelRemoval = null)}
                        >Cancel</Button
                    >
                {:else}
                    <Button
                        variant="ghost"
                        onclick={() => (confirmingModelRemoval = selectedModelInfo?.id ?? null)}
                    >
                        Remove model
                    </Button>
                {/if}
            {/if}
        </div>
    </div>
</fieldset>
