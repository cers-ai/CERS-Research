from __future__ import annotations

from chengsi_worker.reports import export_report

CONTENT = """# 城市绿地与热环境

## 摘要

已有资料提示绿地覆盖与局地温度存在关联 [Wang 等, 2024](https://doi.org/10.1000/example)。

| 指标 | 数值 |
|---|---|
| 样本 | 96 |

- 需要补充全文证据
- 需要复核实验条件
"""


def test_exports_docx_and_pdf(tmp_path) -> None:
    docx_path = tmp_path / "report.docx"
    result = export_report(
        {
            "content": CONTENT,
            "format": "docx",
            "path": str(docx_path),
            "title": "阶段报告",
        }
    )
    assert result["ok"] is True
    assert docx_path.is_file()
    assert docx_path.stat().st_size > 1000

    from docx import Document

    text = "\n".join(paragraph.text for paragraph in Document(str(docx_path)).paragraphs)
    assert "Wang 等, 2024（https://doi.org/10.1000/example）" in text

    pdf_path = tmp_path / "report.pdf"
    result = export_report(
        {
            "content": CONTENT,
            "format": "pdf",
            "path": str(pdf_path),
            "title": "阶段报告",
        }
    )
    assert result["ok"] is True
    assert pdf_path.is_file()
    assert pdf_path.stat().st_size > 1000


def test_rejects_empty_report(tmp_path) -> None:
    result = export_report(
        {
            "content": "   ",
            "format": "docx",
            "path": str(tmp_path / "empty.docx"),
            "title": "空报告",
        }
    )
    assert result["ok"] is False
