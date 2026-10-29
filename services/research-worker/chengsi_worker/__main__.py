"""Sidecar 入口最小骨架（M0 将替换为真实 IPC 服务）。

当前只负责解释器自检，便于 Spike 01 验证打包后的运行时版本。
"""

from __future__ import annotations

import json
import os
import sys

from . import __version__, assert_supported_python
from .server import serve


def _option(args: list[str], name: str) -> str | None:
    if name not in args:
        return None
    index = args.index(name)
    if index + 1 >= len(args):
        return None
    return args[index + 1]


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    try:
        assert_supported_python()
    except RuntimeError as exc:
        print(f"[chengsi-worker] {exc}", file=sys.stderr)
        return 78  # EX_CONFIG

    if "--serve" in args:
        # 优先使用环境变量，避免会话令牌出现在进程命令行中。
        token = os.environ.get("CERS_SIDECAR_TOKEN") or _option(args, "--token")
        if not token:
            print(
                "[chengsi-worker] --serve 需要 CERS_SIDECAR_TOKEN 环境变量",
                file=sys.stderr,
            )
            return 78
        return serve(token)

    if "--self-check" in args:
        print(
            json.dumps(
                {
                    "ok": True,
                    "python": ".".join(str(part) for part in sys.version_info[:3]),
                    "version": __version__,
                    "contractVersion": 1,
                },
                ensure_ascii=False,
            )
        )
        return 0

    print(
        "[chengsi-worker] 请使用 --serve --token <token> 启动服务，"
        "或使用 --self-check 验证运行时。",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
