<script lang="ts">
    import "./overlay.css";
    import { getCurrentWindow } from "@tauri-apps/api/window";
    import { SvelteMap } from "svelte/reactivity";
    import { Channel } from "@tauri-apps/api/core";
    import { Alert, Button, Input } from "anasthasia";
    import { onMountAsync } from "$lib/lifecycle";
    import {
        commands,
        type AppFeed,
        type AppSettings,
        type SessionStatus,
    } from "../types/bindings";

    const FADE_AFTER_MS = 4000;
    const CLEAR_AFTER_FADE_MS = 700;
    const ERROR_VISIBLE_MS = 4500;
    type CompleteAppSettings = Required<AppSettings>;

    const LOADING_LABELS: Partial<Record<SessionStatus, string>> = {
        starting: "Starting…",
        loading: "Loading model…",
        buffering: "Buffering audio…",
        reconnecting: "Reconnecting…",
        stopping: "Stopping…",
    };

    let sessionState: SessionStatus = $state("idle");
    let streamUrl = $state("");
    let errorText = $state("");

    // Caption appearance, mirrored from settings.
    let fontSize = $state(36);
    let fontWeight = $state(600);
    let fontFamily = $state("system-ui, sans-serif");
    let textColor = $state("#ffffff");
    let backingColor = $state("rgba(0, 0, 0, 0.72)");
    let subtitleOffsetMs = $state(0);
    let clickThrough = $state(true);

    // Rolling caption lines: newest at the bottom, at most two on screen so a
    // fast final never flash-replaces the previous one before it can be read.
    type CaptionLine = { id: number; text: string; final: boolean; dimmed: boolean };
    const MAX_LINES = 2;
    let lines: CaptionLine[] = $state([]);
    let nextLineId = 0;
    const lineTimers = new SvelteMap<number, ReturnType<typeof setTimeout>>();

    let card: HTMLDivElement | undefined = $state();

    let mode = $derived.by<"input" | "loading" | "live" | "error">(() =>
        errorText
            ? "error"
            : sessionState === "running"
              ? "live"
              : sessionState === "idle" || sessionState === "failed"
                ? "input"
                : "loading",
    );
    let loadingLabel = $derived(LOADING_LABELS[sessionState] ?? "Working…");
    // Draggable while taking input, or while live with click-through turned off.
    let draggable = $derived(mode === "input" || (mode === "live" && !clickThrough));

    let errorTimer: ReturnType<typeof setTimeout> | undefined;

    // Click-through only while showing live captions AND the user wants it; the
    // bar stays interactive otherwise (input, or live-but-unlocked for dragging).
    $effect(() => {
        void getCurrentWindow().setIgnoreCursorEvents(mode === "live" && clickThrough);
    });

    // Keep the URL field focused whenever the bar is waiting for input.
    $effect(() => {
        if (mode === "input") card?.querySelector("input")?.focus();
    });

    onMountAsync(async () => {
        document.documentElement.classList.add("overlay-document");
        document.body.classList.add("overlay-document");
        void getCurrentWindow().setFocus();
        // Register one persistent channel for all backend → bar pushes (state,
        // captions, errors, settings). Channels deliver reliably to this window
        // where broadcast events did not.
        const feed = new Channel<AppFeed>();
        feed.onmessage = handleFeed;
        await commands.connectFeed(feed);

        const snapshot = await commands.getAppSnapshot();
        if (snapshot.status === "ok") {
            applySettings(snapshot.data.settings);
            sessionState = snapshot.data.sessionState;
            streamUrl = snapshot.data.streamUrl ?? "";
        }
        return [
            () => {
                clearCaption();
                clearTimeout(errorTimer);
            },
        ];
    });

    // Dispatches the channel messages pushed by the backend.
    function handleFeed(message: AppFeed) {
        switch (message.type) {
            case "State":
                sessionState = message.state;
                if (message.state === "idle" || message.state === "failed") clearCaption();
                break;
            case "Settings":
                applySettings(message.settings);
                break;
            case "Subtitle":
                schedule(() => showLine(message.text, true));
                break;
            case "Partial": {
                const text = [message.stable_text, message.unstable_text]
                    .filter(Boolean)
                    .join(" ")
                    .trim();
                if (!text) return;
                schedule(() => showLine(text, false));
                break;
            }
            case "Clear":
                clearCaption();
                break;
            case "Error":
                showError(message.message);
                break;
        }
    }

    function applySettings(settings: AppSettings) {
        const complete = settings as CompleteAppSettings;
        fontSize = complete.fontSizePx;
        fontWeight = complete.fontWeight;
        fontFamily = complete.fontFamily;
        textColor = complete.textColor;
        backingColor = hexToRgba(complete.backgroundColor, complete.backgroundOpacity);
        subtitleOffsetMs = complete.subtitleOffsetMs;
        clickThrough = complete.clickThrough;
    }

    function hexToRgba(hex: string, alpha: number): string {
        const value = hex.replace("#", "");
        const r = parseInt(value.slice(0, 2), 16) || 0;
        const g = parseInt(value.slice(2, 4), 16) || 0;
        const b = parseInt(value.slice(4, 6), 16) || 0;
        return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }

    function schedule(apply: () => void) {
        const delay = Math.max(0, subtitleOffsetMs);
        if (delay === 0) apply();
        else setTimeout(apply, delay);
    }

    // A partial keeps updating the newest (non-final) line in place; a final
    // seals it. The next caption then starts a fresh line below.
    function showLine(text: string, final: boolean) {
        const last = lines.at(-1);
        if (last && !last.final) {
            last.text = text;
            last.final = final;
            last.dimmed = false;
            armExpiry(last.id);
        } else {
            lines.push({ id: nextLineId, text, final, dimmed: false });
            armExpiry(nextLineId);
            nextLineId += 1;
            while (lines.length > MAX_LINES) dropLine(lines[0].id);
        }
    }

    function armExpiry(id: number) {
        clearTimeout(lineTimers.get(id));
        lineTimers.set(
            id,
            setTimeout(() => {
                const line = lines.find((candidate) => candidate.id === id);
                if (!line) return;
                line.dimmed = true;
                lineTimers.set(
                    id,
                    setTimeout(() => dropLine(id), CLEAR_AFTER_FADE_MS),
                );
            }, FADE_AFTER_MS),
        );
    }

    function dropLine(id: number) {
        clearTimeout(lineTimers.get(id));
        lineTimers.delete(id);
        const index = lines.findIndex((candidate) => candidate.id === id);
        if (index !== -1) lines.splice(index, 1);
    }

    function clearCaption() {
        for (const timer of lineTimers.values()) clearTimeout(timer);
        lineTimers.clear();
        lines.splice(0, lines.length);
    }

    function showError(message: string) {
        clearCaption();
        errorText = message;
        clearTimeout(errorTimer);
        errorTimer = setTimeout(() => (errorText = ""), ERROR_VISIBLE_MS);
    }

    async function start() {
        const url = streamUrl.trim();
        if (!url) return;
        // Optimistic so the bar shows progress immediately; the per-session Channel
        // drives every state change from here on.
        sessionState = "starting";
        const result = await commands.startSession(url);
        if (result.status === "error") {
            sessionState = "idle";
            showError(result.error);
        }
    }

    function onUrlKeydown(event: KeyboardEvent) {
        if (event.key === "Enter") void start();
    }

    function onWindowKeydown(event: KeyboardEvent) {
        if (event.key === "Escape") void commands.quitApp();
    }

    function openSettings() {
        void commands.showWindow("settings");
    }

    function startDrag(event: MouseEvent) {
        if (event.button !== 0 || !draggable) return;
        const target = event.target as HTMLElement;
        if (target.closest("input, button, a, [role='button']")) return;
        void getCurrentWindow().startDragging();
    }

    type ResizeCorner = "NorthWest" | "NorthEast" | "SouthWest" | "SouthEast";
    function startResize(event: MouseEvent, corner: ResizeCorner) {
        event.preventDefault();
        event.stopPropagation();
        void getCurrentWindow().startResizeDragging(corner);
    }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<!-- The bar is a drag surface while unlocked; a frameless window has no keyboard
     reposition equivalent, so the non-interactive-element a11y rule doesn't apply. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<main
    class="bar select-none"
    class:is-live={mode === "live"}
    class:is-draggable={draggable}
    onmousedown={startDrag}
>
    {#if mode === "input"}
        <div class="bar-card" bind:this={card}>
            <div class="bar-row">
                <div class="bar-field">
                    <Input
                        placeholder="Paste a YouTube link and press Enter…"
                        bind:value={streamUrl}
                        onkeydown={onUrlKeydown}
                    />
                </div>
                <Button variant="primary" onclick={start} disabled={!streamUrl.trim()}>Start</Button
                >
                <button
                    class="bar-icon"
                    type="button"
                    onclick={openSettings}
                    aria-label="Settings"
                    title="Settings">⚙</button
                >
            </div>
            <p class="bar-hint">Drag to position · ⌘-drag a corner to resize · Esc quits</p>
        </div>

        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
            class="resize-handle nw"
            onmousedown={(event) => startResize(event, "NorthWest")}
        ></div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
            class="resize-handle ne"
            onmousedown={(event) => startResize(event, "NorthEast")}
        ></div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
            class="resize-handle sw"
            onmousedown={(event) => startResize(event, "SouthWest")}
        ></div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
            class="resize-handle se"
            onmousedown={(event) => startResize(event, "SouthEast")}
        ></div>
    {:else if mode === "loading"}
        <div class="bar-card bar-status">
            <span class="spinner" aria-hidden="true"></span>
            <span>{loadingLabel}</span>
        </div>
    {:else if mode === "error"}
        <Alert variant="danger" live="assertive" class="bar-alert">{errorText}</Alert>
    {:else if lines.length}
        <div
            class="caption-stack"
            style:--subtitle-size={`${fontSize}px`}
            style:--subtitle-weight={fontWeight}
            style:--subtitle-family={fontFamily}
            style:--subtitle-color={textColor}
            style:--subtitle-backing={backingColor}
        >
            {#each lines as line, index (line.id)}
                <p
                    class="caption"
                    class:is-live={!line.final}
                    class:is-dim={line.dimmed}
                    class:is-previous={index < lines.length - 1}
                >
                    <span>{line.text}</span>
                </p>
            {/each}
        </div>
    {/if}
</main>
