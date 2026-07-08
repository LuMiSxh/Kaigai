import type { SessionStatus } from "../types/bindings";

export type SessionTone = "idle" | "busy" | "live" | "error";

export interface SessionMeta {
    label: string;
    tone: SessionTone;
}

const META: Record<SessionStatus, SessionMeta> = {
    idle: { label: "Idle", tone: "idle" },
    starting: { label: "Starting", tone: "busy" },
    loading: { label: "Loading model", tone: "busy" },
    buffering: { label: "Buffering", tone: "busy" },
    running: { label: "Live", tone: "live" },
    reconnecting: { label: "Reconnecting", tone: "busy" },
    stopping: { label: "Stopping", tone: "busy" },
    failed: { label: "Failed", tone: "error" },
};

export function sessionMeta(state: SessionStatus): SessionMeta {
    return META[state] ?? { label: state, tone: "idle" };
}

/** Whether the session is in a transient state where Start should stay disabled. */
export function isBusy(state: SessionStatus): boolean {
    return sessionMeta(state).tone === "busy";
}
