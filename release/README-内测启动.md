# 内测启动说明

## 启动方式

解压后双击:

```text
启动 Personal Conductor.cmd
```

不要直接双击 `bin/conductor-desktop.exe`。当前代码仍保留开发阶段的默认根目录回退，启动脚本会把 `CONDUCTOR_ROOT` 设置为当前 release 目录，保证配置、SQLite 和日志写到本包内的 `state/`。

## 配置 LLM

首次启动脚本会从 `state-template/config.json` 复制一份到 `state/config.json`。

需要聊天/主动对话能力时，编辑:

```text
state/config.json
```

把 `llm.apiKey` 从 `null` 改成内部可用 key，并把 `apiKeySet` 改为 `true`。不配置 key 时，桌宠和任务面板仍可启动，但 LLM 对话会失败或提示配置。

## 内测限制

- 这是便携内测包，不是正式安装包。
- 不会自动开机启动。
- 不会默认启用 Claude/Codex hook。
- WebView2 依赖系统环境；Windows 11 通常已具备，Windows 10 如无法启动请先安装 Microsoft Edge WebView2 Runtime。
- hook 功能仍建议只在开发机验证，正式分发前需要补完整安装引导和用户授权流程。
