<script lang="ts">
    import { Input, Panel, Select } from "anasthasia";
    import InfoTip from "$lib/InfoTip.svelte";
    import { settingOptions } from "$lib/settings-options";
    import type { AppSettings } from "../../types/bindings";

    let { settings = $bindable() }: { settings: Required<AppSettings> } = $props();
</script>

<Panel title="Stream access">
    <p class="panel-lead panel-lead-spaced">
        Configure this only for member-only or age-restricted streams.
    </p>
    <div class="settings-rows">
        <div class="settings-row">
            <div class="settings-row-label">
                <span class="anasthasia-label">Cookie source</span>
                <InfoTip text="Use browser or cookies.txt authentication for restricted streams." />
            </div>
            <Select options={settingOptions.cookie} bind:value={settings.cookieMode} />
        </div>
        {#if settings.cookieMode === "browser"}
            <div class="settings-row">
                <div class="settings-row-label"><span class="anasthasia-label">Browser</span></div>
                <Select options={settingOptions.browser} bind:value={settings.browser} />
            </div>
            <div class="settings-row">
                <div class="settings-row-label">
                    <span class="anasthasia-label">Profile</span>
                    <InfoTip text="Leave blank to use the default profile." />
                </div>
                <Input bind:value={settings.browserProfile} placeholder="Default" />
            </div>
        {:else if settings.cookieMode === "file"}
            <div class="settings-row">
                <div class="settings-row-label">
                    <span class="anasthasia-label">Cookie file</span>
                </div>
                <Input bind:value={settings.cookieFile} placeholder="/path/to/cookies.txt" />
            </div>
        {/if}
    </div>
</Panel>
