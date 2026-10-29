# @cers/contracts · 跨进程契约 v1

React（TypeScript）、Rust（Tauri）与 Python Sidecar 共享的唯一契约来源。

## 文件

| 路径 | 作用 |
|---|---|
| `schemas/domain.schema.json` | **唯一事实源**；JSON Schema 2020-12 |
| `ts/domain.ts` | TypeScript 类型镜像（camelCase 线协议） |
| `python/cers_contracts/domain.py` | Pydantic v2 镜像（属性 snake_case，别名 camelCase） |
| `../../scripts/check-contract-parity.mjs` | 三端枚举一致性检查 |

## 规则

- 线协议：camelCase。磁盘 `.cers` 与 SQLite：snake_case。
- 时间：ISO 8601 UTC，带 `Z`。
- ID：`<前缀>_<26 位小写 Crockford ULID>`。
- 破坏性变更提升 `schemaVersion` 并写 ADR；新增可选字段保持 v1。
- 任何一端不得定义同义枚举或字符串别名。

## 使用

```bash
npm run check:contracts
```

Python 侧安装（M0）：

```bash
uv pip install -e packages/contracts/python
```

## 未冻结范围

Provider 请求/响应、SourceAdapter 归一化记录、ReportBlock/AST、SQLite DDL、PermissionGrant scope、Tool Registry、DOCX/PDF 中间格式，分别在 M0–M5 按同一流程补充。
