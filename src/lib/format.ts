const BYTE_UNITS = ["B", "KB", "MB", "GB"] as const;

/** Human-readable byte size, e.g. `147951465` → `"141 MB"`. */
export function formatBytes(bytes: number): string {
    if (!bytes || bytes < 0) return "0 B";
    const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
    return `${(bytes / 1024 ** exponent).toFixed(exponent > 1 ? 1 : 0)} ${BYTE_UNITS[exponent]}`;
}

/** Transfer rate, e.g. `1500000` → `"1.4 MB/s"`. */
export function formatRate(bytesPerSecond: number): string {
    return `${formatBytes(bytesPerSecond)}/s`;
}

/** Countdown label from a remaining-seconds estimate. */
export function formatEta(seconds: number | null): string {
    if (seconds == null) return "Calculating ETA";
    if (seconds < 60) return `${seconds}s remaining`;
    return `${Math.floor(seconds / 60)}m ${seconds % 60}s remaining`;
}
