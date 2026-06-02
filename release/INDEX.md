# Release v0.1.0 — 内部测试版

> 打包时间: 2026-05-27
> 版本: 0.1.0 (MVP)
> 用途: 内部使用，无需脱敏

---

## 文件清单

```
release/
├── INDEX.md                    ← 本文件
├── 打包清单.md                  ← 打包步骤和检查项
├── bin/                        ← 编译产物 (需构建后填入)
│   ├── conductor-desktop.exe   ← Tauri 桌面应用
│   └── conductor.exe           ← CLI 工具 (hook handler)
├── resources/                  ← 运行时资源
│   ├── live2d/
│   │   ├── core/
│   │   │   └── live2dcubismcore.min.js
│   │   └── hiyori/
│   │       └── hiyori_pro_en/
│   │           └── runtime/    ← Live2D 模型运行时
│   └── avatar/                 ← 桌宠形象素材
│       └── default.mp4         ← 默认待机动画
├── state-template/             ← 首次运行状态模板
│   ├── config.json             ← 配置模板 (需填入 API key)
│   └── conductor.sqlite        ← 空数据库 (含 migration)
└── docs/                       ← 文档副本
    ├── PRD.md
    ├── 当前架构说明-用户操作流程-20260526.md
    ├── 交接文档-桌宠渲染与主动对话优化-20260527.md
    └── 交接文档-桌宠工具能力与记忆工作区-20260526.md
```

---

## MVP 功能范围

### 已验证可运行
- ✅ Tauri 桌面壳 (透明窗口、穿透点击、系统托盘)
- ✅ 桌宠窗口 (320x420、浮动/展开双模式、3-tab 布局)
- ✅ 任务面板 (列表/详情、pass/skip/reject 操作)
- ✅ 聊天面板 (消息发送/接收、对话历史)
- ✅ 设置面板 (配置编辑)
- ✅ Live2D 渲染 (Hiyori 模型、状态映射)
- ✅ Claude Code Hook 集成 (12 事件、任务自动创建/更新)
- ✅ Codex Hook 集成 (4 事件)
- ✅ 感知层 (前台窗口监控、空闲检测、进程白名单)
- ✅ 通知决策 (Smart Monitor + fallback)
- ✅ 情绪系统 (7 MoodZone、衰减、事件触发)
- ✅ 好感度系统 (5 阶段、迟滞、每日衰减)
- ✅ 主动对话 (5 触发器：文档写作/编码/多Agent/定时/事件)
- ✅ SQLite 持久化 (13+ 表、migration)
- ✅ CLI 子命令 (hook、list、show、pass、skip、reject、proposal、sub、agent、team、chat)

### 已实现但需进一步测试
- 🧪 Smart Monitor (LLM 驱动通知决策，需 LLM 接通后验证)
- 🧪 Persona 系统 (已接入 chat，需主观体验验证)
- 🧪 主动对话触发 (需长时间运行观察)

### 未实现 (后续版本)
- ❌ File 工具 (file.glob/read/write/edit)
- ❌ Web 工具 (web.search/fetch)
- ❌ Office 工具 (占位实现)
- ❌ QipaoGirl 自定义形象 (需 Cubism Editor 手工操作)
- ❌ 多形象情绪素材图

---

## 环境要求

| 项目 | 要求 |
|------|------|
| OS | Windows 10/11 (x64) |
| 运行时 | 无需额外运行时 (Tauri 自包含) |
| LLM API | OpenAI 兼容 API (当前配置: ark-code-latest via volces.com) |
| 磁盘 | ~300 MB (应用 + 资源) |

---

## 首次运行

1. 解压 release 包到任意目录
2. 编辑 `state-template/config.json`，填入 LLM API key
3. 运行 `bin/conductor-desktop.exe`
4. 桌宠出现在屏幕右下角
5. 右键托盘图标可打开任务面板和聊天面板

---

## 已知问题

| 编号 | 说明 | 优先级 |
|------|------|--------|
| BUG-001 | 桌宠聊天框未打通 LLM，需与后端 chat.rs 对接 | P1 |
| BUG-002 | 右键面板无法关闭、布局溢出 | P1 |
| - | notify_decider.rs 为死代码 (已被 smart_monitor 替代) | P3 |
| - | office 工具为占位实现 | P3 |
| - | Runtime::new() 22+ 处未统一 | P3 |
