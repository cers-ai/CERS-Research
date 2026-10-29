# ADR-0005：Sidecar 使用回环 HTTP + 每进程随机令牌

- 状态：Accepted
- 日期：2026-10-28
- 决策者：技术负责人
- 关联：技术方案 §3.2、§3.3；功能开发需求 FND-02、SEC-02

## 背景

首版需要 Rust 宿主监管 Python 研究进程：启动、健康检查、崩溃重启、退出清理，且不能把 Sidecar 暴露到局域网。备选方案为命名管道、stdio JSON-RPC、`127.0.0.1` HTTP。

## 决策

1. Sidecar **只绑定 `127.0.0.1:0`**，由操作系统分配空闲端口；禁止 `0.0.0.0`。
2. Rust 每次启动生成 **32 字节随机令牌**，通过环境变量 `CERS_SIDECAR_TOKEN` 注入，不写入命令行、日志或项目文件。
3. 启动握手：Python 在 stdout 输出**一行** JSON `{port, pid, python, version, contractVersion}`；stdout 只用于握手，日志全部走 stderr。
4. 所有请求必须带 `Authorization: Bearer <token>`；缺失或错误返回 401。
5. 端点：`GET /health`、`POST /shutdown`。后续研究端点必须做 schema 校验、请求体上限与超时。
6. Rust `SidecarManager` 负责：启动握手、健康检查、异常退出检测、自动重启、退出时 `POST /shutdown` 并等待；Windows 下强制终止使用 `taskkill /T /F` 清理进程树。
7. WebView 不得直连 Sidecar；所有调用经 Rust 转发与权限校验。

## 理由

- HTTP/JSON 调试成本低，便于 M0 快速验证；后续可直接承载 SSE 流式事件。
- 回环 + 随机令牌 + 仅 Rust 可调用，满足首版本机单用户边界。
- 环境变量传令牌避免 `Get-CimInstance CommandLine` 等工具直接读到令牌。

## 已知限制

- 同一操作系统用户下的恶意进程仍可能读取环境变量或本机回环端口；首版信任边界是“同一用户”。若威胁模型升级，改用带 ACL 的命名管道或 Job Object 隔离。
- Windows 下 venv 的 `python.exe` 可能再启动基础解释器，因此必须按进程树终止；打包版本将直接使用内置解释器。
- 当前 `/health` 与 `/shutdown` 是 M0 最小端点；任务执行端点尚未实现。

## 验证

- Python 测试：回环绑定、令牌缺失 401、health、shutdown。
- Rust 测试：启动、健康检查、杀掉 Sidecar 后自动重启且 PID 变化、退出后无残留进程。
- 手工验证：`tauri dev` 启动后日志出现 `GET /health 200`；关闭窗口后 `python -m chengsi_worker --serve` 进程消失。
