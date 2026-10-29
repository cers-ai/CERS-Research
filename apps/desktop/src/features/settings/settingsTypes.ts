import type { ConnectionTestResult, GlobalSettings } from "../../lib/ipc";

export interface SettingsHandlers {
  onPersist: (next: GlobalSettings) => void;
  onTest: (kind: "model" | "source", id: string) => void;
  onSetKey: (
    kind: "model" | "source",
    id: string,
    name: string,
    hasKey: boolean,
  ) => void;
  testingKey: string | null;
  testResults: Record<string, ConnectionTestResult>;
}

export function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "操作失败，请稍后重试。";
}
