const DEFAULT_FONT_FAMILY =
    "system-ui, 'Hiragino Kaku Gothic ProN', 'Yu Gothic', Meiryo, sans-serif";

export const settingOptions = {
    ytDlpSource: [
        { value: "managed", label: "Managed by Kaigai" },
        { value: "system", label: "System install (PATH)" },
    ],
    task: [
        { value: "translate", label: "Translate to English" },
        { value: "transcribe", label: "Transcribe source language" },
    ],
    captionMode: [
        { value: "stable", label: "Stable · recommended" },
        { value: "live", label: "Live · lower latency" },
    ],
    language: [
        { value: "ja", label: "Japanese" },
        { value: "ko", label: "Korean" },
        { value: "zh", label: "Chinese" },
        { value: "en", label: "English" },
        { value: "auto", label: "Auto-detect" },
    ],
    chunk: [
        { value: "adaptive", label: "Adaptive pauses" },
        { value: "fixed", label: "Fixed timing" },
    ],
    vadSensitivity: [
        { value: "high", label: "High · quieter voices" },
        { value: "balanced", label: "Balanced · recommended" },
        { value: "strict", label: "Strict · less noise" },
    ],
    cookie: [
        { value: "none", label: "None" },
        { value: "browser", label: "Browser profile" },
        { value: "file", label: "cookies.txt" },
    ],
    browser: [
        { value: "firefox", label: "Firefox" },
        { value: "safari", label: "Safari" },
        { value: "chrome", label: "Chrome" },
        { value: "edge", label: "Edge" },
        { value: "brave", label: "Brave" },
    ],
    fontFamily: [
        { value: DEFAULT_FONT_FAMILY, label: "System / Gothic (default)" },
        { value: "'Geist Sans', system-ui, sans-serif", label: "Geist Sans" },
        {
            value: "'Hiragino Mincho ProN', 'Yu Mincho', 'Noto Serif JP', serif",
            label: "Mincho (serif)",
        },
        {
            value: "'Hiragino Maru Gothic ProN', 'Quicksand', system-ui, sans-serif",
            label: "Rounded",
        },
        { value: "'JetBrains Mono', ui-monospace, monospace", label: "Monospace" },
    ],
};
