import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatRate } from "./format";

describe("formatBytes", () => {
    it("handles zero and negatives", () => {
        expect(formatBytes(0)).toBe("0 B");
        expect(formatBytes(-5)).toBe("0 B");
    });

    it("scales across units", () => {
        expect(formatBytes(512)).toBe("512 B");
        expect(formatBytes(1024)).toBe("1 KB");
        expect(formatBytes(147_951_465)).toBe("141.1 MB");
        expect(formatBytes(1_533_763_059)).toBe("1.4 GB");
    });
});

describe("formatRate", () => {
    it("appends a per-second suffix", () => {
        expect(formatRate(0)).toBe("0 B/s");
        expect(formatRate(1_500_000)).toBe("1.4 MB/s");
    });
});

describe("formatEta", () => {
    it("returns a placeholder when unknown", () => {
        expect(formatEta(null)).toBe("Calculating ETA");
    });

    it("formats sub-minute and minute durations", () => {
        expect(formatEta(45)).toBe("45s remaining");
        expect(formatEta(125)).toBe("2m 5s remaining");
    });
});
