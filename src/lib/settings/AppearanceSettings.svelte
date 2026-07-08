<script lang="ts">
    import { Input, Panel, Select, Toggle } from "anasthasia";
    import InfoTip from "$lib/InfoTip.svelte";
    import { settingOptions } from "$lib/settings-options";
    import type { AppSettings } from "../../types/bindings";

    let { settings = $bindable() }: { settings: Required<AppSettings> } = $props();
</script>

<Panel title="Subtitle overlay">
    <div class="settings-rows">
        <div class="settings-row">
            <div class="settings-row-label"><span class="anasthasia-label">Font</span></div>
            <Select options={settingOptions.fontFamily} bind:value={settings.fontFamily} />
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Size &amp; weight</span>
                <InfoTip text="Glyph size and weight of the rendered subtitles." />
            </div>
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
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Colors</span>
                <InfoTip text="Text and background colors for the overlay." />
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
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">
                    Opacity <em class="opacity-badge"
                        >{Math.round(settings.backgroundOpacity * 100)}%</em
                    >
                </span>
                <InfoTip text="Transparency of the subtitle background." />
            </div>
            <label class="slider-field">
                <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.05"
                    style:--slider-progress={`${settings.backgroundOpacity * 100}%`}
                    bind:value={settings.backgroundOpacity}
                />
            </label>
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Timing offset</span>
                <InfoTip text="Delay subtitles when they arrive before the video." />
            </div>
            <Input
                label="ms"
                type="number"
                min="-10000"
                max="10000"
                step="100"
                bind:value={settings.subtitleOffsetMs}
            />
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Always on top</span>
                <InfoTip text="Keep the overlay above other windows." />
            </div>
            <Toggle bind:checked={settings.alwaysOnTop} />
        </div>
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Click through while live</span>
                <InfoTip text="Let mouse input pass through the subtitle overlay." />
            </div>
            <Toggle bind:checked={settings.clickThrough} />
        </div>
    </div>
</Panel>
