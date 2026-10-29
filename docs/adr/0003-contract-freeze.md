# ADR-0003：跨进程契约以 JSON Schema 为唯一事实源，线协议统一 camelCase

- 状态：Accepted
- 日期：2026-10-28
- 决策者：技术负责人
- 关联：技术方案 §3.3、§5；功能开发需求 §5；`docs/契约冻结.md`

## 背景

原始文档同时存在以下不一致：

- `SourceCoverage` 有 `citation/abstract` 与 `metadata_only/abstract_only` 两套值；
- `ClaimStatus` 技术方案 6 值、功能需求 4 值；
- 报告变更只有一套状态，无法区分"提案整体"与"单项"；
- 任务事件使用 snake_case，图提案类型使用 camelCase；
- 资产缺少 `relocated` 状态。

如果不在写代码前统一，React/Rust/Python 三端会各自实现一套，后续只能靠转换层打补丁。

## 决策

1. **JSON Schema 为唯一事实源**，TS 与 Pydantic 为其镜像，三端枚举由脚本校验一致。
2. **线协议统一 camelCase**（Tauri command、Sidecar IPC、JobEvent、GraphProposal）。
3. **磁盘格式保持 snake_case**（`.cers`、SQLite、结构化日志），由各端映射。
4. 枚举取并集并记录 UI 中文标签，具体裁决见 `docs/契约冻结.md` §3。
5. 破坏性变更必须提升 `schemaVersion` 并新增 ADR；新增可选字段保持 v1。

## 理由

- 前端是最大调用面，camelCase 减少映射噪音；Rust serde 与 Pydantic alias 各只需一处配置。
- 磁盘 snake_case 与已有 `.cers` 示例和 SQL 习惯一致，避免破坏文件格式。
- 三端一致性脚本成本极低，能在 CI 阶段阻止漂移。

## 后果

- 不允许任何端定义"临时同义枚举"。
- 新增枚举值必须同 PR 更新三份实现与冻结文档。
- M2/M5 的 Provider、SourceAdapter、ReportBlock Schema 必须按同一流程补充，而不是散落在业务代码。

## 验证

- `npm run check:contracts` 通过；
- 每个边界有非法输入拒绝测试；
- PR 模板要求列出契约变更。
