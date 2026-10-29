# 澄思学术桌面版（Chengsi Academic）

本地优先的 AI 科研工作台：以研究问题为牵引，用四类节点组织科研图，在浮动工作区内完成文献、实验与报告，并把正式成果沉淀到统一研究资产库。

## 当前状态

**M0 第一个纵向切片已落地。**

- 需求、技术方案、视觉规范已对齐并冻结关键口径。
- 跨进程契约 v1 已落地，三端一致性可自动校验。
- 桌面工程：Tauri 2 + React 18 + TypeScript + Vite。
- 已实现：打开本地 Workspace、创建 `.cers`、初始化 `.cers-data/project.sqlite`、唯一根节点、受监管的 Python 3.13 Sidecar（回环 HTTP + 随机令牌 + 健康检查 + 崩溃重启 + 退出清理）、项目概况空态与运行时设置页。
- 已实现：Sidecar 可用 PyInstaller 打包为独立可执行文件，并作为 Tauri 资源进入安装包。
- M1 第一切片：只读扫描与资产索引（分类、搜索、缺失标记）、React Flow 四类节点画布、人工新增节点、两类关系（防重复/防分解环）、节点位置持久化、浮动节点工作区（拖动/缩放/收起/关闭）。
- M1 画布补全：节点编辑、节点删除（根节点保护）、连线删除、单步撤销、画布视口持久化。
- 资产全文检索：文本类文件正文抽取、SQLite FTS5（trigram，中英文子串）、命中片段高亮、正文预览。
- 模型与来源配置中心：Provider/Model 双层配置、角色路由、系统凭据库、6 个学术源、9 个新闻源、代理与隐私策略、真实连通测试。
- 下一步：在应用内填入 DeepSeek API Key 并完成真实模型调用；随后接入研究节点 Agent 与 PDF/DOCX 解析。

## 文档地图

| 文档 | 作用 | 优先级 |
|---|---|---|
| [产品需求规格说明书](0.需求/1.澄思学术_产品需求规格说明书.md) | 产品行为、确认边界、资产与安全规则 | 业务基线 |
| [桌面交互 Demo](0.需求/2.澄思学术_桌面交互Demo.html) | 仅交互参考；localStorage/模拟数据禁止进入正式产品 | 参考 |
| [技术方案 V1.2](0.需求/3.澄思学术_桌面版技术方案.md) | 架构、数据模型、技术门槛、Spike | 实现依据 |
| [视觉设计规范](0.需求/4.视觉设计规范.md) | 颜色、字号、间距、组件与动效 | UI 唯一依据 |
| [功能开发需求 V1.1](0.需求/5.功能开发需求.md) | FR-ID、M0–M6、GP-01–GP-18、DoD | **发布判定基线** |
| [契约冻结](docs/契约冻结.md) | 枚举、命名、状态与冲突裁决 | 三端契约 |
| [开发规范](docs/开发规范.md) | 编码、测试、Git、CI、安全、DoD | 提交约束 |
| [开发前准备清单](docs/开发前准备清单.md) | 环境实测、已完成项、M0 待办 | 动工检查 |
| [M0 验收记录](docs/M0-验收记录.md) | 第一个纵向切片的验证证据与已知边界 | 验收追溯 |
| [M1 验收记录](docs/M1-验收记录.md) | 扫描、资产库、画布与浮动工作区的验证证据 | 验收追溯 |
| [M2 配置中心](docs/M2-配置中心.md) | 模型、学术源、新闻源配置与密钥填写说明 | 配置基线 |
| [ADR](docs/adr/) | 架构决策记录 | 决策追溯 |

冲突时：产品行为以 PRD 为准；UI 以视觉规范为准；发布判定以 GP-01–GP-18 为准；跨进程字段以 `packages/contracts` 为准。

## 技术栈与版本

| 层 | 技术 | 版本 |
|---|---|---|
| 桌面壳 | Tauri 2 | 锁定 patch，见 `Cargo.lock` |
| 前端 | React + TypeScript + Vite + @xyflow/react | Node 22 LTS |
| 宿主 | Rust | 1.95.0（`rust-toolchain.toml`） |
| 科研执行 | Python Sidecar | **CPython 3.13.x**（uv 锁定） |
| 存储 | SQLite + FTS5 | WAL + 外键 + 迁移 |
| 契约 | JSON Schema + TS + Pydantic | v1 |

## 目录

```text
apps/desktop/               React UI + Tauri/Rust 宿主
services/research-worker/   Python Sidecar（Agent、模型、来源、解析、实验、报告）
packages/contracts/         跨进程契约
packages/ui/                设计 Token
scripts/                    校验与开发脚本
docs/                       规范、契约、ADR
0.需求/                     产品与设计基线（只读）
```

## 快速开始

```bash
# 1. Python 3.13（不是系统 3.14）
uv python install 3.13
uv python pin 3.13
uv sync --directory services/research-worker --extra dev

# 2. 前端依赖
npm install

# 3. 契约三端一致性
npm run check:contracts

# 4. 启动桌面应用（Tauri）
npm run dev

# 5. 打包 Sidecar 与 Windows 安装包（NSIS）
npm run build:sidecar
npm run bundle:win
```

安装包输出：`apps/desktop/src-tauri/target/release/bundle/nsis/`。

只调试界面时可运行 `npm --workspace @cers/desktop run dev:web`，但打开文件夹与 Sidecar 自检只在桌面窗口中可用。

## 不可协商的规则

1. 只有四类节点（总课题/研究/实验/报告）和两种关系（分解实线 / 关联虚线）。
2. 智能分解必须用户确认；确认前不得写入正式图。
3. 只有四类交互卡；`ResearchAction` 不是第五类卡片。
4. Rust 是项目数据的唯一写入方；React 不碰 SQLite，Python 不写正式库。
5. 密钥只进 OS 凭据库，不进 Workspace、日志、导出包。
6. 原件只读；派生文件记录来源与哈希；新实验运行不覆盖旧运行。
7. 来源覆盖与结论支持是两个正交维度；无全文不得生成页码。
8. 报告 AI 修改走逐项 diff + 确认；不自动覆盖用户编辑。
9. 模拟数据、预置图表、未执行脚本不得标为真实结果。
10. 发布判定只认 GP-01–GP-18 的真实运行证据。

## 下一步

按 [开发前准备清单](docs/开发前准备清单.md) 完成工具链与 Spike 01–04，然后进入 M0。详细任务拆分以《功能开发需求》的 FR-ID 为单位建立 Issue。
