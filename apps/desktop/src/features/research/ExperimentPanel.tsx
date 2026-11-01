import { useEffect, useMemo, useState } from "react";

import {
  listAssets,
  listExperimentRuns,
  runExperiment,
  type AssetItem,
  type ExperimentRun,
} from "../../lib/ipc";

interface ExperimentPanelProps {
  nodeId: string;
  initialGoal: string;
}

export function ExperimentPanel({ nodeId, initialGoal }: ExperimentPanelProps) {
  const [assets, setAssets] = useState<AssetItem[]>([]);
  const [runs, setRuns] = useState<ExperimentRun[]>([]);
  const [assetId, setAssetId] = useState("");
  const [goal, setGoal] = useState(initialGoal);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [target, setTarget] = useState("");
  const [predictors, setPredictors] = useState<string[]>([]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([listAssets(), listExperimentRuns(nodeId)])
      .then(([assetList, runList]) => {
        if (cancelled) {
          return;
        }
        const datasets = assetList.filter(
          (asset) =>
            asset.kind === "dataset" &&
            /\.(csv|tsv)$/i.test(asset.relativePath),
        );
        setAssets(datasets);
        setAssetId((current) => current || datasets[0]?.id || "");
        setRuns(runList);
        setSelectedRunId((current) => current || runList[0]?.id || null);
      })
      .catch((cause) => {
        if (!cancelled) {
          setError(toMessage(cause));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [nodeId]);

  const execute = async () => {
    if (!assetId) {
      setError("请先选择一个 CSV/TSV 数据资产。");
      return;
    }

    setRunning(true);
    setError(null);
    try {
      const run = await runExperiment(nodeId, assetId, goal);
      setRuns((current) => [run, ...current]);
      setSelectedRunId(run.id);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setRunning(false);
    }
  };

  const selected = runs.find((run) => run.id === selectedRunId) ?? runs[0] ?? null;
  const output = selected?.outputs;
  const numericColumns = useMemo(
    () =>
      (output?.columns ?? [])
        .filter((column) => column.type === "number")
        .map((column) => column.name),
    [output],
  );
  const numericSignature = numericColumns.join("|");

  const defaultPredictors = (targetName: string): string[] => {
    const blocked = new Set<string>();
    for (const pair of output?.correlations ?? []) {
      if (Math.abs(pair.r) < 0.999) {
        continue;
      }
      if (pair.x === targetName) {
        blocked.add(pair.y);
      } else if (pair.y === targetName) {
        blocked.add(pair.x);
      }
    }
    return numericColumns.filter(
      (name) => name !== targetName && !blocked.has(name),
    );
  };

  useEffect(() => {
    if (numericColumns.length < 2) {
      return;
    }
    if (!numericColumns.includes(target)) {
      const preferred = numericColumns.includes("cai_ratio_pct")
        ? "cai_ratio_pct"
        : numericColumns[0];
      setTarget(preferred);
      setPredictors(defaultPredictors(preferred));
      return;
    }
    if (predictors.length === 0) {
      setPredictors(defaultPredictors(target));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [numericSignature]);

  const fitRegression = async () => {
    if (!assetId || !target || predictors.length === 0) {
      setError("请选择因变量和至少一个自变量。");
      return;
    }
    setRunning(true);
    setError(null);
    try {
      const run = await runExperiment(nodeId, assetId, goal, target, predictors);
      setRuns((current) => [run, ...current]);
      setSelectedRunId(run.id);
    } catch (cause) {
      setError(toMessage(cause));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="experiment-panel">
      <div className="exp-toolbar">
        <select
          value={assetId}
          aria-label="选择数据资产"
          onChange={(event) => setAssetId(event.target.value)}
        >
          {assets.length === 0 && <option value="">没有可用的 CSV/TSV 资产</option>}
          {assets.map((asset) => (
            <option key={asset.id} value={asset.id}>
              {asset.name} · {asset.relativePath}
            </option>
          ))}
        </select>
        <span className="tag tag--muted">内置 CSV 分析</span>
      </div>

      <textarea
        className="exp-goal"
        value={goal}
        rows={2}
        placeholder="说明要验证或分析什么，例如：比较绿地比例与地表温度的关系。"
        onChange={(event) => setGoal(event.target.value)}
      />

      <div className="row">
        <button
          type="button"
          className="btn btn--primary btn--compact"
          disabled={running || loading || !assetId}
          onClick={() => void execute()}
        >
          {running ? "分析中…" : "运行分析"}
        </button>
        <span className="muted">
          只读取数据文件，不修改原始输入；每次运行保留独立记录。
        </span>
      </div>

      {error && <div className="canvas-error">{error}</div>}
      {loading && <p className="muted">正在读取数据资产与历史运行…</p>}
      {!loading && assets.length === 0 && (
        <p className="hint">
          还没有 CSV/TSV 数据资产。请先在「资产」页扫描包含数据文件的 Workspace。
        </p>
      )}

      {runs.length > 0 && (
        <div className="exp-history">
          <span className="muted">历史运行</span>
          {runs.map((run, index) => (
            <button
              key={run.id}
              type="button"
              className={`choice ${selected?.id === run.id ? "choice--active" : ""}`}
              onClick={() => setSelectedRunId(run.id)}
            >
              R{runs.length - index} · {formatTime(run.createdAt)}
            </button>
          ))}
        </div>
      )}

      {selected && output && (
        <div className="exp-result">
          <div className="section-head">
            <strong>运行结果</strong>
            <span
              className={`tag ${selected.state === "completed" ? "tag--success" : "tag--warning"}`}
            >
              {selected.state === "completed" ? "已完成" : "失败"}
            </span>
            {output.summary?.truncated && (
              <span className="tag tag--warning">仅分析前 20 万行</span>
            )}
          </div>

          {selected.state === "failed" ? (
            <p className="canvas-error">
              {selected.error || output.message || "实验分析失败。"}
            </p>
          ) : (
            <>
              <div className="exp-metrics">
                <Metric label="数据行" value={output.summary?.rows ?? 0} />
                <Metric label="列数" value={output.summary?.columns ?? 0} />
                <Metric label="数值列" value={output.summary?.numericColumns ?? 0} />
                <Metric label="缺失单元格" value={output.summary?.missingCells ?? 0} />
              </div>

              {output.correlations && output.correlations.length > 0 && (
                <>
                  <h4 className="exp-section-title">数值列相关性</h4>
                  <div className="exp-corr-list">
                    {output.correlations.slice(0, 6).map((item) => (
                      <span className="tag tag--muted" key={`${item.x}-${item.y}`}>
                        {item.x} ↔ {item.y}：r = {item.r.toFixed(3)}（n={item.n}）
                      </span>
                    ))}
                  </div>
                </>
              )}

              {numericColumns.length >= 2 && (
                <div className="exp-regression">
                  <h4 className="exp-section-title">线性回归（标准化系数）</h4>
                  <div className="exp-regression-controls">
                    <label className="form-field form-field--inline">
                      <span>因变量</span>
                      <select
                        value={target}
                        aria-label="回归因变量"
                        onChange={(event) => {
                          const next = event.target.value;
                          setTarget(next);
                          setPredictors(defaultPredictors(next));
                        }}
                      >
                        {numericColumns.map((name) => (
                          <option key={name} value={name}>
                            {name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <div className="exp-predictor-list">
                      <span className="muted">自变量</span>
                      {numericColumns
                        .filter((name) => name !== target)
                        .map((name) => (
                          <label key={name} className="exp-predictor">
                            <input
                              type="checkbox"
                              checked={predictors.includes(name)}
                              onChange={(event) =>
                                setPredictors((current) =>
                                  event.target.checked
                                    ? [...current, name]
                                    : current.filter((item) => item !== name),
                                )
                              }
                            />
                            {name}
                          </label>
                        ))}
                    </div>
                  </div>
                  <div className="row">
                    <button
                      type="button"
                      className="btn btn--compact"
                      disabled={running || !target || predictors.length === 0}
                      onClick={() => void fitRegression()}
                    >
                      {running ? "拟合中…" : "拟合线性模型"}
                    </button>
                    <span className="muted">
                      标准化 β 可比较独立解释力；VIF 大于 5 表示共线性较强。
                    </span>
                  </div>

                  {(output.groups ?? []).length > 0 && (
                    <div className="exp-group-result">
                      <h4 className="exp-section-title">分组均值</h4>
                      {output.groups?.map((summary) => (
                        <div key={summary.column} className="exp-group-block">
                          <p className="muted">
                            {summary.column} × {summary.target}
                          </p>
                          <div className="exp-table-wrap">
                            <table className="exp-table">
                              <thead>
                                <tr>
                                  <th>组</th>
                                  <th>n</th>
                                  <th>均值</th>
                                  <th>标准差</th>
                                </tr>
                              </thead>
                              <tbody>
                                {summary.groups.map((group) => (
                                  <tr key={group.name}>
                                    <td>{group.name}</td>
                                    <td>{group.n}</td>
                                    <td>{formatNumber(group.mean)}</td>
                                    <td>{formatNumber(group.std)}</td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}

                  {output.regression && (
                    <div className="exp-regression-result">
                      {output.regression.ok ? (
                        <>
                          <div className="exp-metrics">
                            <Metric label="样本 n" value={output.regression.n ?? 0} />
                            <Metric
                              label="R²"
                              value={Number((output.regression.r2 ?? 0).toFixed(4))}
                            />
                            <Metric
                              label="调整 R²"
                              value={Number(
                                (output.regression.adjustedR2 ?? 0).toFixed(4),
                              )}
                            />
                            <Metric
                              label="自变量数"
                              value={(output.regression.predictors ?? []).length}
                            />
                          </div>
                          <div className="exp-table-wrap">
                            <table className="exp-table">
                              <thead>
                                <tr>
                                  <th>自变量</th>
                                  <th>标准化 β</th>
                                  <th>原尺度 β</th>
                                  <th>标准误</th>
                                  <th>t</th>
                                  <th>VIF</th>
                                </tr>
                              </thead>
                              <tbody>
                                {(output.regression.coefficients ?? []).map((item) => (
                                  <tr key={item.name}>
                                    <td>{item.name}</td>
                                    <td>{formatNumber(item.stdBeta)}</td>
                                    <td>{formatNumber(item.beta)}</td>
                                    <td>{formatNumber(item.stdError)}</td>
                                    <td>{formatNullable(item.t)}</td>
                                    <td>{formatNullable(item.vif)}</td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </div>
                          {output.regression.note && (
                            <p className="hint">{output.regression.note}</p>
                          )}
                        </>
                      ) : (
                        <p className="canvas-error">
                          {output.regression.message ?? "回归失败。"}
                        </p>
                      )}
                    </div>
                  )}
                </div>
              )}

              <h4 className="exp-section-title">列统计</h4>
              <div className="exp-table-wrap">
                <table className="exp-table">
                  <thead>
                    <tr>
                      <th>列</th>
                      <th>类型</th>
                      <th>缺失</th>
                      <th>均值</th>
                      <th>标准差</th>
                      <th>最小</th>
                      <th>最大</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(output.columns ?? []).map((column) => (
                      <tr key={column.name}>
                        <td>{column.name}</td>
                        <td>{column.type === "number" ? "数值" : "文本"}</td>
                        <td>{column.missing}</td>
                        <td>{formatNumber(column.mean)}</td>
                        <td>{formatNumber(column.std)}</td>
                        <td>{formatNumber(column.min)}</td>
                        <td>{formatNumber(column.max)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {output.preview && (
                <>
                  <h4 className="exp-section-title">数据预览</h4>
                  <div className="exp-table-wrap">
                    <table className="exp-table">
                      <thead>
                        <tr>
                          {output.preview.headers.map((header) => (
                            <th key={header}>{header}</th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {output.preview.rows.map((row, rowIndex) => (
                          <tr key={rowIndex}>
                            {output.preview?.headers.map((_, columnIndex) => (
                              <td key={columnIndex}>{row[columnIndex] ?? ""}</td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="exp-metric">
      <small>{label}</small>
      <strong>{value}</strong>
    </div>
  );
}

function formatNullable(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : value.toFixed(3);
}

function formatNumber(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "—";
  }
  return Number.isInteger(value) ? String(value) : value.toFixed(3);
}

function formatTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleTimeString("zh-CN", { hour12: false });
}

function toMessage(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  return "实验操作失败，请稍后重试。";
}
