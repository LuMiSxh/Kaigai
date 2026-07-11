<script lang="ts">
    import "./developer.css";
    import { tick } from "svelte";
    import { Badge, Button, Panel, Toggle } from "anasthasia";
    import AppHeader from "$lib/AppHeader.svelte";
    import { onMountAsync } from "$lib/lifecycle";
    import {
        commands,
        events,
        type AppSnapshot,
        type DeveloperLogEntry,
        type MetricsEvent,
        type SessionStatus,
    } from "../../types/bindings";

    const MAX_VISIBLE_LOGS = 500;
    const DEV_BUILD = import.meta.env.DEV;

    let snapshot: AppSnapshot | null = $state(null);
    let sessionState: SessionStatus = $state("idle");
    let metrics: MetricsEvent | null = $state(null);
    let entries: DeveloperLogEntry[] = $state([]);
    let lastLogId = 0;
    let paused = $state(false);
    let autoScroll = $state(true);
    let logViewport: HTMLDivElement | undefined = $state();

    const BACKEND_LABELS: Record<string, string> = {
        coreml: "Core ML",
        metal: "Metal",
        cpu: "CPU",
    };
    let backend = $derived.by(
        () => BACKEND_LABELS[snapshot?.settings.inferenceBackend ?? ""] ?? "Metal",
    );
    let rtf = $derived.by(() => metrics?.realtimeFactor ?? null);
    let rtfTone = $derived(rtf == null ? "" : rtf > 1 ? "warn" : "good");
    let lagTone = $derived.by(() => ((metrics?.captureLagMs ?? 0) > 2000 ? "warn" : ""));

    async function refreshLogs() {
        if (paused) return;
        const incoming = await commands.getRecentLogs(lastLogId || null);
        if (!incoming.length) return;
        lastLogId = incoming.at(-1)?.id ?? lastLogId;
        // Trim in a single pass: drop the overflow from the front, then append.
        const overflow = entries.length + incoming.length - MAX_VISIBLE_LOGS;
        if (overflow > 0) entries.splice(0, overflow);
        entries.push(...incoming);
        if (autoScroll) {
            await tick();
            logViewport?.scrollTo({ top: logViewport.scrollHeight });
        }
    }

    onMountAsync(async () => {
        if (!DEV_BUILD) {
            await commands.hideWindow("developer");
            return [];
        }
        const interval = window.setInterval(() => void refreshLogs(), 500);

        const snapshotResult = await commands.getAppSnapshot();
        if (snapshotResult.status === "ok") {
            snapshot = snapshotResult.data;
            sessionState = snapshotResult.data.sessionState;
        }
        await refreshLogs();
        return [
            () => window.clearInterval(interval),
            await events.metricsEvent.listen((event) => {
                metrics = event.payload;
            }),
            await events.sessionStateEvent.listen((event) => {
                sessionState = event.payload.state;
            }),
            await events.settingsUpdatedEvent.listen((event) => {
                if (snapshot) snapshot = { ...snapshot, settings: event.payload.settings };
            }),
        ];
    });

    function clearVisibleLogs() {
        entries = [];
    }
</script>

{#snippet stat(label: string, value: string, sub: string, tone = "")}
    <div class="stat" data-tone={tone}>
        <span class="stat-label">{label}</span>
        <span class="stat-value">{value}</span>
        <span class="stat-sub">{sub}</span>
    </div>
{/snippet}

{#if DEV_BUILD}
    <main class="app-shell developer-shell">
        <AppHeader kicker="Kaigai runtime" title="Developer Console" state={sessionState} />
        <p class="developer-note">
            Local diagnostics. Authentication data and signed media URLs are redacted before
            logging.
        </p>

        <Panel label="Telemetry" title="Runtime metrics" class="telemetry-panel">
            <div class="stat-grid">
                {@render stat(
                    "Model",
                    snapshot?.settings.model ?? "—",
                    snapshot?.settings.modelPath ? `${backend} backend` : "PCM only",
                )}
                {@render stat(
                    "Chunk mode",
                    snapshot?.settings.chunkMode ?? "—",
                    metrics?.reason ?? "No cut yet",
                )}
                {@render stat(
                    "Audio chunk",
                    `${metrics?.chunkMs ?? 0} ms`,
                    `${metrics?.audioMs ?? 0} ms processed`,
                )}
                {@render stat(
                    "Real-time factor",
                    rtf == null ? "—" : `${rtf.toFixed(2)}×`,
                    metrics?.inferenceMs == null
                        ? "Inactive"
                        : `${metrics.inferenceMs} ms inference`,
                    rtfTone,
                )}
                {@render stat(
                    "Inference queue",
                    `${metrics?.queueDelayMs ?? 0} ms`,
                    `${metrics?.queueDepth ?? 0} pending`,
                    (metrics?.queueDepth ?? 0) > 0 ? "warn" : "",
                )}
                {@render stat(
                    "PCM delivery",
                    `${metrics?.pcmGapMs ?? 0} ms`,
                    "Gap before latest read",
                )}
                {@render stat(
                    "Capture lag",
                    `${metrics?.captureLagMs ?? 0} ms`,
                    "Behind wall clock",
                    lagTone,
                )}
            </div>
        </Panel>

        <Panel label="Logs" title="Application log" class="developer-log-panel">
            {#snippet actions()}
                <Badge variant="mono">{entries.length}</Badge>
                <Toggle label="Auto-scroll" bind:checked={autoScroll} />
                <Button
                    size="sm"
                    variant="ghost"
                    onclick={() => (paused = !paused)}
                    aria-pressed={paused}
                >
                    {paused ? "Resume" : "Pause"}
                </Button>
                <Button size="sm" variant="ghost" onclick={clearVisibleLogs}>Clear</Button>
            {/snippet}

            <div class="log-surface">
                <div class="log-viewport" bind:this={logViewport}>
                    {#if entries.length}
                        {#each entries as entry (entry.id)}
                            <div class="log-line">
                                <span>{entry.id.toString().padStart(4, "0")}</span>
                                <code>{entry.message}</code>
                            </div>
                        {/each}
                    {:else}
                        <div class="log-empty">
                            {paused ? "Log stream paused." : "Waiting for log entries…"}
                        </div>
                    {/if}
                </div>
            </div>
        </Panel>
    </main>
{/if}
