import type { ConnectionTestResult } from "../../lib/ipc";

export function TestResultView({ result }: { result?: ConnectionTestResult }) {
  if (!result) {
    return null;
  }

  return (
    <div className={`test-result ${result.ok ? "is-ok" : "is-error"}`}>
      <div className="test-result-head">
        <strong>{result.ok ? "连接成功" : "连接失败"}</strong>
        <span>
          {result.statusCode ? `HTTP ${result.statusCode} · ` : ""}
          {result.latencyMs} ms
        </span>
      </div>
      <p>{result.message}</p>
      {result.detail && (
        <details>
          <summary>查看响应</summary>
          <pre>{result.detail}</pre>
        </details>
      )}
    </div>
  );
}
