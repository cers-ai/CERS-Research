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
