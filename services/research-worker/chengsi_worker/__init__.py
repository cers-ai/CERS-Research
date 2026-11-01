"""澄思学术研究执行 Sidecar。

解释器版本由 ADR-0002 锁定为 CPython 3.13.x；启动自检不通过时直接失败，
不得静默降级到系统 Python。
"""

from __future__ import annotations

import sys

__version__ = "0.0.1"

REQUIRED_PYTHON = (3, 13)


class UnsupportedPythonError(RuntimeError):
    """当前解释器不是交付锁定的版本。"""


def assert_supported_python(version: tuple[int, int] | None = None) -> None:
    """校验解释器主次版本；Sidecar 入口必须在启动时调用。"""
    actual = version or (sys.version_info.major, sys.version_info.minor)
    if actual != REQUIRED_PYTHON:
        required = ".".join(str(part) for part in REQUIRED_PYTHON)
        found = ".".join(str(part) for part in actual)
        raise UnsupportedPythonError(
            f"澄思学术 Sidecar 需要 Python {required}.x，当前为 {found}.x；"
            "请使用仓库根目录的 .python-version 与 uv，不要把系统 Python 作为运行时。"
        )
