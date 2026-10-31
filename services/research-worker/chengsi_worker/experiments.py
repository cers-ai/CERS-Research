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
