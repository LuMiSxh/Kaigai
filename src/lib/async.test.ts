import { describe, expect, it, vi } from "vitest";
import { whileBusy } from "./async";

describe("whileBusy", () => {
    it("clears busy state after success", async () => {
        const setBusy = vi.fn();

        await expect(whileBusy(setBusy, async () => "done")).resolves.toBe("done");
        expect(setBusy.mock.calls).toEqual([[true], [false]]);
    });

    it("clears busy state after failure", async () => {
        const setBusy = vi.fn();
        const error = new Error("IPC unavailable");

        await expect(
            whileBusy(setBusy, async () => {
                throw error;
            }),
        ).rejects.toBe(error);
        expect(setBusy.mock.calls).toEqual([[true], [false]]);
    });
});
