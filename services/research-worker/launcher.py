"""PyInstaller 入口：把 Sidecar 打包为独立可执行文件。"""

from chengsi_worker.__main__ import main

if __name__ == "__main__":
    raise SystemExit(main())
