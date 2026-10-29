# ADR-0001：桌面技术栈采用 Tauri 2 + React + Rust + Python Sidecar

- 状态：Accepted
- 日期：2026-10-28
- 决策者：技术负责人
- 关联：技术方案 §2.3、§20.1 Spike 01

## 背景

首版要求 Windows 11 优先、本地优先、无云端账号依赖，同时需要文件/权限/事务的强边界和 Python 科研生态（模型、检索、解析、科学计算）。候选为 Tauri 2 与 Electron。

## 决策

采用 **Tauri 2 + React 18 + TypeScript + Rust Core + Python Sidecar + SQLite（FTS5）**。

- React 只做界面与临时状态。
- Rust 是 Workspace 路径、权限、SQLite 事务、Sidecar 生命周期的唯一权威。
- Python 承担 Agent Harness、模型适配、学术/新闻源、文档解析、实验执行。
- 前端通过 typed Tauri commands 调用，不直连 Sidecar。

## 理由

- 安装包与运行时通常更轻，权限边界更明确。
- Rust 的权限门禁可以成为真实执行边界，而不是提示词约束。
- Python 生态适合科研任务，且可作为独立进程隔离崩溃。

## 后果

- 必须解决 Sidecar 打包、启动、退出、崩溃恢复、杀毒兼容。
- 需要维护三端契约与 IPC 校验，开发成本高于纯 Web。
- Python 解释器必须随包分发并锁定版本（见 ADR-0002）。

## 验证关口

M0 的 Spike 01 必须在未安装开发工具的 Windows 机器上完成：安装、启动、打开项目、调用 Sidecar、杀掉 Sidecar 后 UI 不崩溃、卸载不删 Workspace。若 Tauri 的 Sidecar 分发/杀毒兼容无法达标，再启动 Electron 备选评估；**不同时维护两套桌面壳**。
