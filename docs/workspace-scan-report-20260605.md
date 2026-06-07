# 工作区快速扫描报告

> 扫描时间：2026-06-05  
> 扫描模式：Agent Team 自动扫描  
> 扫描范围：`I:\personal-agent`（排除 node_modules、target、.git）

---

## 1. 项目概览

| 项目 | 值 |
|------|-----|
| **名称** | Personal Conductor（个人指挥官） |
| **定位** | Windows 桌宠式桌面搭子，常驻桌面 + 聊天/任务/工作台面板 |
| **架构风格** | Tauri v2 桌面壳 + React 前端 + Rust 核心 + SQLite 本地存储 + Live2D 桌宠渲染 |
| **开发状态** | 内测阶段（v0.1.0），密集迭代中 |
| **当前活跃度** | 高 — 过去 2 周有 15+ 份新架构/方案文档，且 Conductor 运行时当前有 17 个会话、15 个待审 Hook、6 个待办任务 |

---

## 2. 目录结构全景

```
I:\personal-agent/
├── apps/desktop/                  ← Tauri 桌面壳 + React 前端
│   ├── src/                       ← 59 个 .tsx/.ts 组件文件
│   │   ├── windows/               ← PetWindow, ChatPanel, GoalConsole, TaskDrawer, AgentLanes...
│   │   ├── components/            ← 通用 UI 组件 + 卡片组件
│   │   ├── live2d/                ← Live2DCanvas
│   │   └── ...
│   └── src-tauri/                 ← Tauri Rust 后端
├── crates/
│   ├── conductor-core/            ← 核心运行时（SQLite, 工具, 记忆, 目标流转）
│   ├── conductor-cli/             ← CLI 入口
│   ├── conductor-sense/           ← 前台感知与状态支持
│   ├── model-router-core/         ← 模型路由核心
│   └── model-router-mcp/          ← 模型路由 MCP Server
├── docs/                          ← 70+ 份架构/产品/演进文档
├── tools/                         ← 工具定义
├── skills/                        ← 技能定义
├── scripts/                       ← 构建与打包脚本
├── release/                       ← 便携 zip 骨架
├── state/                         ← 运行时数据（SQLite + config + logs）
├── 2Dworkspace/                   ← Live2D 资产
├── live2d-automation-master/      ← Live2D 自动化框架
├── wiki/                          ← Wiki markdown
├── Cargo.toml                     ← Workspace 根（6 个成员）
├── package.json                   ← 前端依赖
├── rust-toolchain.toml            ← Rust 工具链配置
├── dev.ps1 / start-dev.bat        ← 开发启动脚本
├── README.md / README.zh-CN.md    ← 双语介绍
└── CHANGES.md                     ← 变更日志
```

---

## 3. 技术栈快照

| 层级 | 技术 | 备注 |
|------|------|------|
| **语言** | Rust（主力）、TypeScript/React（前端） | 双语言 |
| **桌面壳** | Tauri v2 | 带托盘、快捷键、多插件 |
| **前端框架** | React + Vite | 59 个组件，覆盖聊天/任务/Live2D |
| **持久化** | SQLite（sqlx + migration） | 对话、任务、配置 |
| **AI 路由** | Axum HTTP server + fastembed | 本地模型推理与路由 |
| **桌宠渲染** | Live2D Cubism | PNG 动态化 + 自动化框架 |
| **进程管理** | codex（自定义 runner） | 多 Agent 会话管理 |
| **工具/技能** | tools/ + skills/ 目录定义 | 自动发现注册 |
| **打包分发** | zip 便携包 | release/ + state-template/ |

---

## 4. 文档状态

共扫描到 **70+ 份 Markdown 文档**，按主题分布：

| 主题类别 | 代表文档 | 说明 |
|---------|---------|------|
| 产品定义 | PRD.md, MVP-实施方案.md | 已完成 |
| 架构设计 | 技术架构.md, 技术栈选型.md | 基础架构就绪 |
| **近期演进（核心）** | UnifiedFinalEvolution, HybridModel与Cairn融合, Chat-Turn state model, 多Agent自治调度, Skill导入方案 | **2026-05 下旬至今在密集推动** |
| Agent 治理 | Agent架构状态机与治理范式, Agent场景状态机现状差距, 状态机驱动的开发范式 | 治理体系日趋成熟 |
| Live2D/桌宠 | 角色设定归档, 表情状态机, 静态PNG动态化方案 | 艺术资产已归档 |
| 派工/任务 | Round1~4 派工文档, 文档同步状态 | 清晰的历史任务分派 |
| 交接/评审 | 多份"交接文档"、"落地状态评估"、"评审" | 团队协作痕迹明显 |

**文档体系评级：良好** — 既有历史沉淀，也有最新的演进方案，索引清晰（INDEX.md）。

---

## 5. 当前开发阶段判断

```
[构思] → [原型] → [MVP] → [内测] → [公开] → [迭代]
                         ▲
                    你在这里
```

**判断依据：**
- ✅ Rust 核心 + Tauri 桌面壳 + React 前端均已可运行
- ✅ Live2D 桌宠渲染已集成
- ✅ 聊天/任务/目标流转核心链路已跑通
- ✅ 便携 zip 打包分发链路可用
- ⏳ **正在攻坚**：AgentTeam 多 Agent 调度、Chat-Turn 状态机收口、Hybrid/Cairn 融合架构
- ❌ 尚未：自动化测试覆盖率不足、CI/CD 未就绪、公开分发未部署

**当前阶段：内测迭代期（v0.1.x），处于架构能力向稳定产品过渡的关键窗口。**

---

## 6. 下一步演进建议

### 建议一：收口 Chat-Turn 状态机与 AgentTeam 调度
文档已有完整方案（Chat-Turn 审计/实现、多 Agent 自治调度），建议集中一个 sprint 完成：
- 将 `docs/` 中 Chat-Turn 相关方案转化为代码实现
- 建立 AgentTeam 生命周期的端到端测试
- 收口后冻结状态机协议一周，避免持续变动

### 建议二：补齐自动测试与 CI 基础
当前 `target-verify-*` 目录显示有验证尝试但未沉淀为 CI：
- 为 conductor-core 增加单元测试（已有 sqlite 层，测试成本低）
- 为前端关键组件（GoalConsole, ChatPanel, AgentLanes）增加 React Testing Library 测试
- 在 `.github/` 中配置 push/PR 触发的基础 CI（`cargo test` + `npm test` + `cargo build`）

### 建议三：将演进文档中的决策沉淀为核心代码注释
当前 `docs/` 有大量高质量决策文档，但代码中缺乏对应的 inline 注释/架构标记：
- 为新模块（AgentTeam、Chat-Turn、Skill 导入）添加 `// Architecture:` 级注释
- 在关键接口上标注对应文档链接
- 降低新参与者（或未来的自己）的回溯成本

### 建议四：评估并推进 HybridModel/Cairn 融合的可行性验证
`HybridModel与Cairn融合方案` 是重要的架构跃迁，建议：
- 用一个小型 PoC（如 mock MCP server）验证方案中的关键假设
- 确定"落地 Padding"的时间窗口，避免影响当前内测节奏
- 输出融合后的模块依赖图，标注影响范围

### 建议五：建立公共发布就绪清单
从内测到公开分发需要检查：
- 打包体积优化（当前 target 目录多份 verify 副本可清理）
- 用户引导/Onboarding 流程（已有 Onboarding.tsx 组件，可完善）
- 卸载/清理逻辑
- Windows 签名准备
- 遥测与崩溃上报（可选）

---

## 附录：扫描信息

| 项目 | 数据 |
|------|------|
| 扫描工具 | Agent Team（scan-reporter）+ 手动审查 |
| 扫描耗时 | ~2 分钟 |
| 总扫描文件数 | ~200+（排除生成目录） |
| 未扫描区域 | node_modules, target, .git, .venv, 二进制文件 |