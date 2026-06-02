# Personal Conductor

[English README](README.md)

![清和桌宠](apps/desktop/public/avatar/document_secretary/shy.png)

Personal Conductor 是一个面向 Windows 的桌宠式桌面搭子。
它把 Tauri 桌面壳、React 前端、Rust 核心、本地 SQLite 状态，以及头像 / Live2D 渲染组合在一起，让一个常驻桌面的助手陪着用户处理编码、任务跟进和工作台对话。

## 它是什么

- 一个常驻桌面的桌宠窗口，带托盘入口
- 一组辅助面板：聊天、任务、设置、工作台
- 一个本地优先的运行时，状态写入 `state/`
- 一套头像与 Live2D 资源，用来表达当前工作状态
- 一条面向 Windows 内测分发的 zip 便携打包链路

## 当前能力定位

- 桌边陪伴：桌宠常驻、托盘唤起、快捷键打开面板
- 工作台协助：在 `workbench` 中承接聊天、目标、任务上下文
- 本地存储：对话、任务、配置等数据默认保存在本机
- 形象驱动：主形象与子形象可配合状态切换
- 便携分发：当前以 zip 便携包为主

## 仓库结构

```text
apps/desktop/          React + Vite 前端与 Tauri 桌面壳
crates/conductor-core/ 核心运行时、SQLite、工具、记忆、目标流转
crates/conductor-cli/  CLI 入口
crates/conductor-sense/前台应用感知与状态支持
docs/                  产品、架构、实现文档
release/               便携版骨架与发布说明
scripts/               构建、打包、辅助脚本
```

## 本地开发

环境要求：

- Windows 10/11 x64
- Rust stable
- Node.js 20+
- npm

启动开发环境：

```powershell
.\dev.ps1
```

常用命令：

```powershell
cd apps\desktop
npm run build

cargo build -p conductor-cli
cargo build -p conductor-desktop
```

## 打包与分发

生成标准便携 zip：

```powershell
.\scripts\package-portable.ps1
```

便携包启动后会先从 `release/state-template/` 初始化，再把新的本地运行数据写入 `state/`。

## 运行数据

以下内容默认只保留在本地，不应提交到仓库：

- `state/conductor.sqlite`
- `state/config.json`
- `state/events.ndjson`
- `state/summaries/`

## 说明

- 当前项目重点是 Windows 桌宠体验。
- 现阶段主打 zip 便携分发，不是正式安装器。
- 对外发包前应清理本地对话记录、向量记忆、运行日志和测试残留。
