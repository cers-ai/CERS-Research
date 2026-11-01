from __future__ import annotations

import pytest

from chengsi_worker.experiments import run_experiment


def test_analyzes_csv_without_modifying_input(tmp_path) -> None:
    path = tmp_path / "data.csv"
    original = "city,green_ratio,temp\nA,0.31,32.1\nB,0.42,30.8\nC,0.55,29.4\n"
    path.write_text(original, encoding="utf-8")

    result = run_experiment({"path": str(path), "goal": "比较绿地比例与温度"})

    assert result["ok"] is True
    assert result["summary"]["rows"] == 3
    assert result["summary"]["columns"] == 3
    assert result["summary"]["numericColumns"] == 2
    assert result["preview"]["headers"] == ["city", "green_ratio", "temp"]
    assert len(result["correlations"]) == 1
    assert result["correlations"][0]["x"] == "green_ratio"
    assert result["correlations"][0]["y"] == "temp"
    assert result["correlations"][0]["r"] == pytest.approx(-0.9904, abs=0.01)
    assert path.read_text(encoding="utf-8") == original


def test_rejects_missing_or_empty_file(tmp_path) -> None:
    missing = run_experiment({"path": str(tmp_path / "missing.csv")})
    assert missing["ok"] is False

    empty = tmp_path / "empty.csv"
    empty.write_text("", encoding="utf-8")
    assert run_experiment({"path": str(empty)})["ok"] is False


def test_fits_standardized_linear_regression_with_vif(tmp_path) -> None:
    path = tmp_path / "regression.csv"
    lines = ["x1,x2,y"]
    for index in range(1, 21):
        x1 = index
        x2 = (index * 7 % 13) + 1.0
        y = 2.0 * x1 - 1.5 * x2 + 4.0 + (0.3 if index % 2 else -0.3)
        lines.append(f"{x1},{x2},{y}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    result = run_experiment(
        {
            "path": str(path),
            "goal": "估计独立解释力",
            "target": "y",
            "predictors": ["x1", "x2"],
        }
    )

    assert result["ok"] is True
    regression = result["regression"]
    assert regression["ok"] is True
    assert regression["n"] == 20
    assert regression["r2"] > 0.99
    assert [item["name"] for item in regression["coefficients"]] == ["x1", "x2"]
    assert regression["coefficients"][0]["stdBeta"] > 0
    assert regression["coefficients"][1]["stdBeta"] < 0
    assert all(item["vif"] is not None for item in regression["coefficients"])


def test_returns_group_summaries_for_text_columns(tmp_path) -> None:
    path = tmp_path / "groups.csv"
    lines = ["group,x,y", "A,1,10", "A,2,12", "B,3,25", "B,4,27", "C,5,40", "C,6,42"]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    result = run_experiment(
        {
            "path": str(path),
            "goal": "分组比较",
            "target": "y",
            "predictors": ["x"],
        }
    )

    assert result["ok"] is True
    groups = result["groups"]
    assert len(groups) == 1
    assert groups[0]["column"] == "group"
    assert [item["name"] for item in groups[0]["groups"]] == ["A", "B", "C"]
    assert groups[0]["groups"][0]["mean"] == pytest.approx(11.0)
