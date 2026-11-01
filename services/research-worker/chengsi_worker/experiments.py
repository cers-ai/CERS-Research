"""首版内置 CSV/TSV 分析（标准库实现，不执行任意脚本）。

只读取数据文件并生成统计摘要、相关系数与预览，不修改输入文件。
"""

from __future__ import annotations

import csv
import math
import statistics
from pathlib import Path
from typing import Any

MAX_FILE_BYTES = 50 * 1024 * 1024
MAX_ROWS = 200_000
MAX_PREVIEW_ROWS = 12
MAX_CORRELATION_COLUMNS = 12


def run_experiment(payload: dict[str, Any]) -> dict[str, Any]:
    path = Path(str(payload.get("path") or ""))
    goal = str(payload.get("goal") or "").strip()

    if not path.is_file():
        return {"ok": False, "message": "数据文件不存在或无法访问。"}
    if path.suffix.lower() not in (".csv", ".tsv"):
        return {"ok": False, "message": "首版只支持 CSV / TSV 数据分析。"}
    if path.stat().st_size > MAX_FILE_BYTES:
        return {"ok": False, "message": "数据文件超过 50 MB，请先拆分或抽样。"}

    delimiter = "\t" if path.suffix.lower() == ".tsv" else ","
    try:
        with path.open("r", encoding="utf-8-sig", errors="replace", newline="") as handle:
            rows = list(csv.reader(handle, delimiter=delimiter))
    except OSError as exc:
        return {"ok": False, "message": f"读取数据文件失败：{exc}"}

    if not rows:
        return {"ok": False, "message": "数据文件为空。"}

    headers = [header.strip() or f"列{index + 1}" for index, header in enumerate(rows[0])]
    if not headers:
        return {"ok": False, "message": "数据文件没有表头。"}

    data_rows = [row for row in rows[1 : MAX_ROWS + 1] if any(cell.strip() for cell in row)]
    truncated = len(rows) - 1 > MAX_ROWS

    columns: list[dict[str, Any]] = []
    numeric_values: dict[str, list[float]] = {}
    missing_cells = 0

    for index, name in enumerate(headers):
        values = [row[index].strip() if index < len(row) else "" for row in data_rows]
        missing = sum(1 for value in values if value == "")
        missing_cells += missing

        parsed: list[float] = []
        for value in values:
            if value == "":
                continue
            try:
                parsed.append(float(value))
            except ValueError:
                parsed = []
                break

        is_numeric = bool(parsed) and len(parsed) + missing == len(values)
        if is_numeric:
            numeric_values[name] = parsed
            columns.append(
                {
                    "name": name,
                    "type": "number",
                    "missing": missing,
                    "count": len(parsed),
                    "mean": statistics.fmean(parsed) if parsed else None,
                    "std": statistics.pstdev(parsed) if len(parsed) > 1 else None,
                    "min": min(parsed) if parsed else None,
                    "max": max(parsed) if parsed else None,
                    "median": statistics.median(parsed) if parsed else None,
                }
            )
        else:
            counts: dict[str, int] = {}
            for value in values:
                if value:
                    counts[value] = counts.get(value, 0) + 1
            top_values = sorted(counts.items(), key=lambda item: item[1], reverse=True)[:5]
            columns.append(
                {
                    "name": name,
                    "type": "text",
                    "missing": missing,
                    "count": len(values) - missing,
                    "topValues": [{"value": value, "count": count} for value, count in top_values],
                }
            )

    correlations = _correlations(numeric_values)
    target = str(payload.get("target") or "").strip()
    predictors = payload.get("predictors")
    if not isinstance(predictors, list):
        predictors = []
    predictors = [str(item).strip() for item in predictors if str(item).strip()]
    regression = None
    groups: list[dict[str, Any]] = []
    if target and predictors:
        regression = _regression(
            headers,
            data_rows,
            numeric_values,
            target,
            predictors,
        )
        groups = _groups(headers, data_rows, numeric_values, target)

    return {
        "ok": True,
        "goal": goal,
        "summary": {
            "rows": len(data_rows),
            "columns": len(headers),
            "numericColumns": len(numeric_values),
            "missingCells": missing_cells,
            "truncated": truncated,
        },
        "columns": columns,
        "correlations": correlations,
        "regression": regression,
        "groups": groups,
        "preview": {
            "headers": headers,
            "rows": [row[: len(headers)] for row in data_rows[:MAX_PREVIEW_ROWS]],
        },
    }


def _correlations(numeric_values: dict[str, list[float]]) -> list[dict[str, Any]]:
    names = list(numeric_values)[:MAX_CORRELATION_COLUMNS]
    results: list[dict[str, Any]] = []
    for left_index, left in enumerate(names):
        for right in names[left_index + 1 :]:
            xs = numeric_values[left]
            ys = numeric_values[right]
            size = min(len(xs), len(ys))
            if size < 3:
                continue
            coefficient = _pearson(xs[:size], ys[:size])
            if coefficient is not None:
                results.append(
                    {
                        "x": left,
                        "y": right,
                        "r": coefficient,
                        "n": size,
                    }
                )
    results.sort(key=lambda item: abs(item["r"]), reverse=True)
    return results


def _pearson(xs: list[float], ys: list[float]) -> float | None:
    count = len(xs)
    if count < 2:
        return None
    mean_x = sum(xs) / count
    mean_y = sum(ys) / count
    numerator = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys, strict=False))
    denominator_x = math.sqrt(sum((x - mean_x) ** 2 for x in xs))
    denominator_y = math.sqrt(sum((y - mean_y) ** 2 for y in ys))
    if denominator_x == 0 or denominator_y == 0:
        return None
    return numerator / (denominator_x * denominator_y)


def _regression(
    headers: list[str],
    data_rows: list[list[str]],
    numeric_values: dict[str, list[float]],
    target: str,
    predictors: list[str],
) -> dict[str, Any]:
    if target not in numeric_values:
        return {"ok": False, "message": f"因变量不是数值列：{target}"}
    predictors = [name for name in predictors if name in numeric_values and name != target]
    if not predictors:
        return {"ok": False, "message": "至少选择一个与因变量不同的数值自变量。"}
    if len(set(predictors)) != len(predictors):
        return {"ok": False, "message": "自变量重复。"}

    index = {name: position for position, name in enumerate(headers)}
    samples: list[tuple[float, list[float]]] = []
    for row in data_rows:
        try:
            y = float(row[index[target]].strip())
            xs = [float(row[index[name]].strip()) for name in predictors]
        except (KeyError, IndexError, ValueError):
            continue
        samples.append((y, xs))
    n = len(samples)
    k = len(predictors)
    if n < k + 2:
        return {
            "ok": False,
            "message": f"完整样本仅 {n} 条，不足以估计 {k} 个自变量。",
        }

    ys = [item[0] for item in samples]
    xs_by_column = [[item[1][j] for item in samples] for j in range(k)]
    mean_y = statistics.fmean(ys)
    sd_y = statistics.pstdev(ys) or 1.0
    means = [statistics.fmean(column) for column in xs_by_column]
    sds = [statistics.pstdev(column) or 1.0 for column in xs_by_column]

    z_y = [(value - mean_y) / sd_y for value in ys]
    z_x = [
        [(value - means[j]) / sds[j] for value in xs_by_column[j]]
        for j in range(k)
    ]

    matrix = [[_dot(z_x[i], z_x[j]) for j in range(k)] for i in range(k)]
    vector = [_dot(z_x[i], z_y) for i in range(k)]
    beta = _solve(matrix, vector)
    if beta is None:
        return {"ok": False, "message": "自变量矩阵奇异，可能存在完全共线。"}

    fitted = [sum(beta[j] * z_x[j][row] for j in range(k)) for row in range(n)]
    residuals = [z_y[row] - fitted[row] for row in range(n)]
    sse = sum(value * value for value in residuals)
    sst = sum(value * value for value in z_y)
    r2 = 1.0 - sse / sst if sst else 0.0
    adjusted = 1.0 - (1.0 - r2) * (n - 1) / (n - k - 1)
    sigma2 = sse / (n - k - 1)
    inverse = _invert(matrix)
    if inverse is None:
        return {"ok": False, "message": "无法计算回归标准误。"}

    vifs = []
    for i in range(k):
        others = [j for j in range(k) if j != i]
        if not others:
            vifs.append(1.0)
            continue
        other_matrix = [[_dot(z_x[j], z_x[m]) for m in others] for j in others]
        other_vector = [_dot(z_x[j], z_x[i]) for j in others]
        other_beta = _solve(other_matrix, other_vector)
        if other_beta is None:
            vifs.append(float("inf"))
            continue
        other_fitted = [
            sum(other_beta[pos] * z_x[j][row] for pos, j in enumerate(others))
            for row in range(n)
        ]
        other_sse = sum((z_x[i][row] - other_fitted[row]) ** 2 for row in range(n))
        vif = 1.0 / (other_sse / n) if other_sse else float("inf")
        vifs.append(vif)

    intercept = mean_y
    coefficients = []
    for j, name in enumerate(predictors):
        standard_error = math.sqrt(max(0.0, sigma2 * inverse[j][j]))
        t_value = beta[j] / standard_error if standard_error else None
        original = beta[j] * sd_y / sds[j]
        intercept -= original * means[j]
        coefficients.append(
            {
                "name": name,
                "beta": original,
                "stdBeta": beta[j],
                "stdError": standard_error,
                "t": t_value,
                "vif": None if math.isinf(vifs[j]) else vifs[j],
            }
        )

    return {
        "ok": True,
        "target": target,
        "predictors": predictors,
        "n": n,
        "r2": r2,
        "adjustedR2": adjusted,
        "intercept": intercept,
        "coefficients": coefficients,
        "note": "标准化 beta 用于比较独立解释力；VIF 大于 5 表示共线性较强。",
    }


def _dot(left: list[float], right: list[float]) -> float:
    return sum(a * b for a, b in zip(left, right, strict=False))


def _solve(matrix: list[list[float]], vector: list[float]) -> list[float] | None:
    size = len(vector)
    work = [row[:] + [vector[index]] for index, row in enumerate(matrix)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(work[row][column]))
        if abs(work[pivot][column]) < 1e-12:
            return None
        work[column], work[pivot] = work[pivot], work[column]
        divisor = work[column][column]
        for cell in range(column, size + 1):
            work[column][cell] /= divisor
        for row in range(size):
            if row == column:
                continue
            factor = work[row][column]
            if factor == 0:
                continue
            for cell in range(column, size + 1):
                work[row][cell] -= factor * work[column][cell]
    return [work[row][size] for row in range(size)]


def _invert(matrix: list[list[float]]) -> list[list[float]] | None:
    size = len(matrix)
    work = [
        row[:] + [1.0 if row_index == column else 0.0 for column in range(size)]
        for row_index, row in enumerate(matrix)
    ]
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(work[row][column]))
        if abs(work[pivot][column]) < 1e-12:
            return None
        work[column], work[pivot] = work[pivot], work[column]
        divisor = work[column][column]
        for cell in range(2 * size):
            work[column][cell] /= divisor
        for row in range(size):
            if row == column:
                continue
            factor = work[row][column]
            if factor == 0:
                continue
            for cell in range(2 * size):
                work[row][cell] -= factor * work[column][cell]
    return [row[size:] for row in work]


def _groups(
    headers: list[str],
    data_rows: list[list[str]],
    numeric_values: dict[str, list[float]],
    target: str,
) -> list[dict[str, Any]]:
    if target not in numeric_values:
        return []
    target_index = headers.index(target)
    summaries: list[dict[str, Any]] = []
    for index, name in enumerate(headers):
        if name in numeric_values:
            continue
        buckets: dict[str, list[float]] = {}
        for row in data_rows:
            if index >= len(row) or target_index >= len(row):
                continue
            label = row[index].strip()
            if not label:
                continue
            try:
                value = float(row[target_index].strip())
            except ValueError:
                continue
            buckets.setdefault(label, []).append(value)
        if len(buckets) < 2 or len(buckets) > 20:
            continue
        groups = [
            {
                "name": label,
                "n": len(values),
                "mean": statistics.fmean(values),
                "std": statistics.pstdev(values) if len(values) > 1 else 0.0,
            }
            for label, values in buckets.items()
        ]
        groups.sort(key=lambda item: item["name"])
        summaries.append({"column": name, "target": target, "groups": groups})
        if len(summaries) >= 3:
            break
    return summaries
