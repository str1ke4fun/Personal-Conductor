# 派工 · Round 2 · 桌面壳与 Live2D 集成

> 目标：**两到三周内**把 Round 1 的 CLI/markdown 形态升级成**真正的桌面应用**——托盘常驻、task panel 弹窗、Live2D 桌宠在屏幕角落动起来、状态切换看得见。
>
> 验收标准：开机后桌宠出现在桌面角落，Claude 完成一次产出后桌宠"抬眼看你"，点击桌宠弹出 task panel，能在面板里 `pass/skip/reject`。
>
> **本轮技术栈决策**（沿用既定路线）：
> - 桌面壳：Tauri v2 + React + TypeScript + Vite。
> - 后端核心：复用 Round 1 的 `conductor-core` crate（**零搬迁**）。
> - 持久化：从 `tasks.json` 文件迁移到 SQLite（`events.ndjson` 保留作为事实来源）。
> - Live2D：PIXI.js + `pixi-live2d-display` + Cubism 4 Web SDK，先用 Hiyori。
>
> **前置依赖**：Round 1 全部 T 完成、`cargo build --release` 通过、Hook 已挂通且能跑通端到端演练。

---

## 0. 约定

- **执行人**：你 + 开发 agent。
- **工作目录**：`I:\personal-agent\`
- **新增 Node 工具链**：Node ≥ 20（仅用于 Tauri 前端构建，不引入 Node 运行时到生产）。
- **Live2D 资源**：先用 `docs/Live2D-个人推进清单.md` 车道 A 跑通 Hiyori，本轮**不阻塞等你的自定义形象**。
- **Round 2 严禁拉进来的东西**：LLM 摘要（仍 Week 2 在 Round 2.5 单独做）、OS hook、Codex、反向写命令、移动端。

---

## 1. 工程骨架变更

```
I:\personal-agent\
├── Cargo.toml                              # 新增 apps/desktop/src-tauri 进 workspace
├── crates\
│   ├── conductor-core\                     # 已有 (Round 1)，本轮新增 SQLite 后端
│   │   └── src\
│   │       ├── db.rs                       # ★ 新增：SQLite 池 + migrations
│   │       └── tasks.rs                    # ★ 改造：从文件后端切到 db 后端
│   └── conductor-cli\                      # 已有，本轮保持兼容（仍能跑 hook stop）
├── apps\
│   └── desktop\                            # ★ 新增：Tauri 桌面壳
│       ├── package.json                    # React + Vite + pixi-live2d-display
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── src\                            # 前端 UI
│       │   ├── main.tsx
│       │   ├── App.tsx
│       │   ├── windows\
│       │   │   ├── PetWindow.tsx           # 桌宠窗口
│       │   │   ├── TaskPanel.tsx           # task 弹窗
│       │   │   └── SettingsWindow.tsx      # 设置
│       │   ├── live2d\
│       │   │   ├── Live2DCanvas.tsx        # PIXI + pixi-live2d-display 封装
│       │   │   └── stateMap.ts             # 4 状态 → motion/expression 映射
│       │   ├── ipc\
│       │   │   └── invoke.ts               # 调 Tauri command 的薄封装
│       │   └── styles\
│       └── src-tauri\                      # Tauri Rust 端
│           ├── Cargo.toml                  # 依赖 conductor-core
│           ├── tauri.conf.json
│           ├── build.rs
│           ├── icons\
│           ├── resources\
│           │   └── live2d\
│           │       └── hiyori\             # Live2D 模型资源
│           └── src\
│               ├── main.rs                 # 入口、窗口、托盘
│               ├── commands.rs             # Tauri command（list/show/pass/...）
│               ├── worker.rs               # 后台 worker：消费 events.ndjson、发状态
│               └── tray.rs                 # 系统托盘菜单
└── state\
    ├── conductor.sqlite                    # ★ 新增主状态库
    ├── tasks.md                            # 仍保留，作为人读镜像
    ├── events.ndjson                       # 仍保留，作为事实来源
    └── summaries\
```

**关键设计点**：
- `conductor-cli` **保持不变**，仍能被 Claude hook 调用——hook 不感知有没有 GUI。
- 新的桌面壳 `apps/desktop/src-tauri` **不替代** CLI，它是另一个二进制 `conductor-desktop.exe`，常驻、有 UI。
- `conductor-core` 同时被 CLI 和桌面壳引用，**只有一份业务逻辑**。

---

## 2. workspace 与依赖

### 2.1 根 `Cargo.toml` 变更

```toml
[workspace]
members = [
  "crates/conductor-core",
  "crates/conductor-cli",
  "apps/desktop/src-tauri",   # ★ 新增
]

[workspace.dependencies]
# 已有依赖保留
sqlx        = { version = "0.7", features = ["runtime-tokio", "sqlite", "chrono", "uuid", "migrate"] }  # ★ 新增
tauri       = { version = "2", features = [] }                                                         # ★ 新增
tauri-plugin-shell        = "2"
tauri-plugin-notification = "2"
tauri-plugin-global-shortcut = "2"
```

### 2.2 `apps/desktop/package.json`

```json
{
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri"
  },
  "dependencies": {
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "pixi.js": "^7.4.0",
    "pixi-live2d-display": "^0.4.0",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-shell": "^2",
    "@tauri-apps/plugin-notification": "^2",
    "@tauri-apps/plugin-global-shortcut": "^2"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "@vitejs/plugin-react": "^4.3.0",
    "typescript": "^5.4.0",
    "vite": "^5.2.0"
  }
}
```

**为什么这套**：
- `pixi.js` 钉 v7（pixi-live2d-display 兼容性最好，v8 还在迁移期）。
- Tauri 插件按需引：shell（拉起外部）/ notification（系统通知）/ global-shortcut（全局快捷键）。
- **不引入 React 状态管理库**——本轮窗口少、组件少，`useState` + Tauri event 够用。

---

## 3. 工作分解（按依赖顺序）

> 开始 T1 之前请确认：
> 1. Round 1 验收通过。
> 2. `docs/Live2D-个人推进清单.md` 车道 A 的 A1–A5 完成（Cubism Editor 装好、Hiyori 在 pixi-live2d-display demo 里能眨眼）。
> 3. `npm` 可用，`tauri info` 能跑（参见 §2.2 前置）。

### T1. 接入 Tauri 工程骨架

**做什么**：

1. 在 `I:\personal-agent\apps\desktop\` 下 `npm create tauri-app@latest .` 选 React + TypeScript + Vite。
2. 把生成的 `src-tauri/` 移动到位、`Cargo.toml` 加入根 workspace。
3. 在 `src-tauri/Cargo.toml` 加 `conductor-core = { path = "../../../crates/conductor-core" }` 引用。
4. 跑 `cargo tauri dev`，确认默认的"Hello Tauri"窗口能弹出来。

**验收**：
- `cargo build --workspace` 通过（CLI 仍能编、桌面壳能编）。
- `cd apps/desktop && npm run tauri dev` 弹出空 Tauri 窗口。
- `target/release/conductor.exe hook stop` 仍能被 Claude hook 调用（回归 Round 1）。

---

### T2. SQLite 后端切换（`conductor-core::db`）

**做什么**：

1. 新增 `conductor-core/src/db.rs`，用 `sqlx` 建一个全局连接池。
2. 新增 `migrations/` 目录，第一份迁移 `0001_init.sql`：

```sql
CREATE TABLE tasks (
  id TEXT PRIMARY KEY,
  source TEXT NOT NULL,
  kind TEXT NOT NULL,
  artifact_file TEXT,
  artifact_anchor TEXT,
  summary_ref TEXT,
  est_minutes INTEGER,
  focus_hint TEXT,
  status TEXT NOT NULL,           -- pending/in_progress/passed/rejected/skipped
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  due_at TEXT,
  snoozed_until TEXT,
  priority TEXT DEFAULT 'normal'
);

CREATE INDEX idx_tasks_status_created ON tasks(status, created_at DESC);

CREATE TABLE notification_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  quiet_until TEXT,
  last_notified_at TEXT,
  pending_minutes INTEGER DEFAULT 0,
  pending_count INTEGER DEFAULT 0
);

INSERT INTO notification_state(id) VALUES (1);
```

3. **改造 `tasks.rs`**：把 `add` / `update` / `load` 从文件后端切到 SQLite，对外 API 签名**不变**。
4. 写一个 **一次性迁移工具** `conductor-cli migrate`：读 `state/tasks.json`（如果存在）→ 导入 SQLite → 把 `tasks.json` 重命名为 `tasks.json.bak`。
5. **保留** `tasks.md` 渲染逻辑，每次写入后从 SQLite 重新渲染。

**验收**：
1. 跑 `conductor migrate`，旧 `tasks.json` 的所有任务都进了 SQLite。
2. `conductor list` 输出与迁移前一致。
3. 跑 `cargo test -p conductor-core db::` 全部通过。
4. 并发写 100 条任务，SQLite 里精确 100 条且无字段错乱。

---

### T3. Tauri commands：把 CLI 能力暴露给前端

**做什么**：

在 `apps/desktop/src-tauri/src/commands.rs` 写 5 个 `#[tauri::command]`：

```rust
#[tauri::command]
async fn list_tasks(only_pending: bool) -> Result<Vec<Task>, String>;
#[tauri::command]
async fn show_task(id: String) -> Result<TaskWithSummary, String>;
#[tauri::command]
async fn pass_task(id: String) -> Result<(), String>;
#[tauri::command]
async fn skip_task(id: String) -> Result<(), String>;
#[tauri::command]
async fn reject_task(id: String) -> Result<(), String>;
```

内部都调 `conductor_core::tasks::*`。错误用 `.map_err(|e| e.to_string())` 转字符串（Tauri command 要求错误可序列化）。

**前端薄封装** `src/ipc/invoke.ts`：

```ts
import { invoke } from '@tauri-apps/api/core';

export const api = {
  listTasks: (onlyPending = true) => invoke<Task[]>('list_tasks', { onlyPending }),
  showTask: (id: string) => invoke<TaskWithSummary>('show_task', { id }),
  passTask: (id: string) => invoke<void>('pass_task', { id }),
  skipTask: (id: string) => invoke<void>('skip_task', { id }),
  rejectTask: (id: string) => invoke<void>('reject_task', { id }),
};
```

**验收**：在 App.tsx 里临时加一个按钮点了能打印 `listTasks()` 结果。

---

### T4. TaskPanel 窗口

**做什么**：

1. `tauri.conf.json` 注册第二个窗口 `task-panel`：

```json
{
  "windows": [
    {
      "label": "pet",
      "url": "index.html#pet",
      "width": 240,
      "height": 320,
      "transparent": true,
      "decorations": false,
      "alwaysOnTop": true,
      "skipTaskbar": true,
      "visible": false
    },
    {
      "label": "task-panel",
      "url": "index.html#tasks",
      "width": 420,
      "height": 560,
      "decorations": true,
      "visible": false,
      "skipTaskbar": true
    }
  ]
}
```

2. `src/App.tsx` 用 `window.location.hash` 分流到不同窗口组件。

3. **`TaskPanel.tsx`** 内容：
   - 顶部："当前 N 条待审 · X 分钟窗口"
   - 列表：每条任务一张卡片，展示 kind / artifact / focus_hint / est_minutes。
   - 每张卡 3 个按钮：✓ Pass / ⏭ Skip / ✗ Reject。
   - 点卡片标题：调 `showTask` → 弹出底部抽屉显示完整 summary markdown。

4. **快捷键**：Tauri global shortcut 注册 `Ctrl+Shift+L`（list） → 显示/隐藏 TaskPanel。

5. **窗口位置**：默认弹在屏幕右侧居中（用 `currentMonitor()` 算）。

**验收**：
- 启动 desktop app，按 `Ctrl+Shift+L`，TaskPanel 弹出，显示真实任务列表。
- 点 ✓ Pass，列表立即少一条，`tasks.md` 同步更新。
- 关闭 TaskPanel，再按快捷键再弹出。

---

### T5. PetWindow 桌宠窗口 + Live2D 渲染

**做什么**：

1. 在 `src-tauri/resources/live2d/hiyori/` 下放好 Hiyori 模型文件（参考 `Live2D-个人推进清单.md` A4）。
2. `tauri.conf.json` 把 resources 打包进去：

```json
{
  "bundle": {
    "resources": ["resources/live2d/**/*"]
  }
}
```

3. `src/live2d/Live2DCanvas.tsx`：

```tsx
import * as PIXI from 'pixi.js';
import { Live2DModel } from 'pixi-live2d-display/cubism4';
import { useEffect, useRef } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { resolveResource } from '@tauri-apps/api/path';

export function Live2DCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const modelRef = useRef<Live2DModel | null>(null);

  useEffect(() => {
    (async () => {
      const app = new PIXI.Application({
        view: canvasRef.current!,
        backgroundAlpha: 0,
        resolution: window.devicePixelRatio,
        width: 240,
        height: 320,
      });
      const modelPath = await resolveResource('resources/live2d/hiyori/hiyori_pro_t10.model3.json');
      const url = convertFileSrc(modelPath);
      const model = await Live2DModel.from(url);
      model.scale.set(0.18);
      model.anchor.set(0.5, 0);
      model.position.set(120, 20);
      app.stage.addChild(model);
      modelRef.current = model;
    })();
  }, []);

  return <canvas ref={canvasRef} style={{ width: '100%', height: '100%' }} />;
}
```

4. `src/windows/PetWindow.tsx` 渲染 `<Live2DCanvas />`，加点击事件：

```tsx
onClick={async () => {
  const win = await WebviewWindow.getByLabel('task-panel');
  if (await win?.isVisible()) await win.hide();
  else await win?.show();
}}
```

5. **窗口配置**（关键）：
   - `transparent: true`
   - `decorations: false`
   - `alwaysOnTop: true`
   - `skipTaskbar: true`
   - 默认位置：屏幕右下角，距边 24px。

**验收**：
- 启动 desktop app，桌宠（Hiyori）出现在屏幕右下角，背景完全透明、能看到桌面壁纸。
- 鼠标悬停不挡其他应用点击（点击穿透在 T6 解决，本步先看到桌宠就算过）。
- 点击桌宠区域，task panel 显示/隐藏。

---

### T6. 点击穿透（Windows）

**做什么**：

Windows 透明窗口默认所有区域都接收点击，遮挡其他应用。我们要做到：**桌宠轮廓内接收点击，其他透明区域穿透**。

策略：**整窗口默认穿透，鼠标 hover 进桌宠轮廓矩形时切回不穿透**。

```rust
// src-tauri/src/main.rs
use tauri::Manager;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

#[tauri::command]
fn set_pet_click_through(window: tauri::WebviewWindow, through: bool) {
    #[cfg(windows)]
    unsafe {
        let hwnd = window.hwnd().unwrap();
        let mut style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if through {
            style |= (WS_EX_LAYERED | WS_EX_TRANSPARENT).0 as isize;
        } else {
            style &= !(WS_EX_TRANSPARENT.0 as isize);
            style |= WS_EX_LAYERED.0 as isize;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
}
```

前端 `PetWindow.tsx` 用 `onMouseEnter` / `onMouseLeave` 触发对应 invoke。或者更精细：监听 PIXI hitArea 内的指针事件。

**MVP 取舍**：本轮做**矩形 hover 切换**就够了，精确轮廓点击穿透留到桌宠形象完成后再调。

**验收**：
- 默认状态，桌面其他应用图标可点击（鼠标不被桌宠拦截）。
- 鼠标移到桌宠矩形内，光标变化（说明窗口接收事件），可点击。
- 移开，再次穿透。

---

### T7. 后台 worker：消费 events.ndjson → 推前端状态

**做什么**：

1. `src-tauri/src/worker.rs` 启动一个 tokio task：
   - 启动时把 `events.ndjson` 已有未消费的事件全部消化一次（用 SQLite 里一个 `events_cursor` 记录已处理到第几行）。
   - 之后用 `notify` crate 监听文件追加，新事件来了就处理。
2. 处理逻辑：
   - 解析事件，若是 `claude/stop` 且对应 task 已通过 hook 入库（Round 1 链路已做），**emit 事件 `task_created` 给所有前端窗口**。
3. 状态机：
   - `idle`：当前 SQLite `tasks where status='pending'` 为空。
   - `new_task`：30 秒内有新 task_created。
   - `pending_review`：pending 累计 est_minutes > 30。
   - `quiet`：`notification_state.quiet_until > now`。
4. 状态变更时 `app_handle.emit_all("pet_state", new_state)`。

**前端 PetWindow.tsx 订阅**：

```tsx
useEffect(() => {
  const unlisten = listen<string>('pet_state', e => {
    modelRef.current?.expression(e.payload);   // surprised / sleep / default
  });
  return () => { unlisten.then(f => f()); };
}, []);
```

**stateMap.ts**：
```ts
export const STATE_TO_EXPR: Record<string, string> = {
  idle: 'default',
  new_task: 'surprised',
  pending_review: 'tap_body',
  quiet: 'sleep',
};
```

**验收**：
- 启动 desktop app，桌宠 idle 状态。
- 在 hook 项目里跑一次 Claude（触发 Stop hook，Round 1 链路完成入库 + 写 events.ndjson）。
- 5 秒内，桌宠切到 `new_task` 表情，task panel 自动多一条。

---

### T8. 系统托盘

**做什么**：

`src-tauri/src/tray.rs`：

```rust
use tauri::{tray::TrayIconBuilder, menu::{Menu, MenuItem}};

pub fn build_tray(app: &tauri::AppHandle) {
    let show = MenuItem::with_id(app, "show", "显示桌宠", true, None::<&str>).unwrap();
    let panel = MenuItem::with_id(app, "panel", "打开 Task 面板", true, Some("CmdOrCtrl+Shift+L")).unwrap();
    let quiet = MenuItem::with_id(app, "quiet", "专注 30 分钟", true, None::<&str>).unwrap();
    let quit  = MenuItem::with_id(app, "quit", "退出", true, None::<&str>).unwrap();
    let menu  = Menu::with_items(app, &[&show, &panel, &quiet, &quit]).unwrap();

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show"  => { /* 显示/隐藏 pet 窗口 */ },
            "panel" => { /* 显示 task-panel */ },
            "quiet" => { /* 写 notification_state.quiet_until = now + 30min */ },
            "quit"  => app.exit(0),
            _ => {}
        })
        .build(app)
        .unwrap();
}
```

**验收**：
- 系统托盘出现一个图标。
- 右键弹菜单，4 项都能跑（点"专注 30 分钟"后桌宠在 30 分钟内切到 `quiet` 表情且不响）。

---

### T9. 系统通知（跨应用打断）

**做什么**：

1. 装 `tauri-plugin-notification`。
2. 在 worker 状态机里判断：
   - 当 `pending_count >= 3` **且** 距上次通知 >= 30 分钟 **且** 不在 quiet → 发系统通知。
   - 通知内容："Conductor · 还有 N 件审阅等你（约 M 分钟）"。
3. 通知点击 → 显示 task-panel。

**验收**：
- 制造 ≥3 个 pending task。
- 30 秒内出现 Windows 系统通知。
- 点击通知，task-panel 弹出。
- 立即再制造一个新 task，**不会**再次通知（冷却生效）。

---

### T10. 端到端总演练 + 打包

**做什么**：

1. `npm run tauri build` 出 release exe。
2. 写一个 `桌面壳启动.bat` 放到启动文件夹（或 Windows "启动" 注册）：
   ```bat
   start "" /B "I:\personal-agent\apps\desktop\src-tauri\target\release\conductor-desktop.exe"
   ```
3. 重启机器，确认开机后桌宠自动出现。

**总演练步骤**：

1. 开机 → 桌宠在右下角，idle 状态。
2. 在文档项目跑 Claude 改一段文字。
3. **期望**：5 秒内桌宠切到 `new_task` 表情；继续跑 1–2 个产出 → 切到 `pending_review`。
4. 点击桌宠 → TaskPanel 弹出 → 看到 3 条任务，每条有 kind/artifact/focus_hint。
5. 点第一条 ✓ Pass → 列表少一条，`tasks.md` 同步打勾。
6. 托盘右键 → "专注 30 分钟" → 桌宠 sleep，30 分钟内不再通知。
7. 30 分钟后回到 idle 或 pending_review，若 pending ≥ 3 弹一次系统通知。

**全部对得上 = Round 2 通过。**

---

## 4. Round 2 严禁扩张

- ❌ 不写 LLM 摘要（Round 2.5 单独）。
- ❌ 不写对话式 chat 子命令（Round 2.5）。
- ❌ 不实现 UserPromptSubmit 真实逻辑（Round 3）。
- ❌ 不接 OS hook / 跨应用感知（Round 3）。
- ❌ 不接反向写命令（Round 3）。
- ❌ 不上自定义 Live2D 形象（先用 Hiyori，自定义形象进 Round 2 之后单独替换贴图，不阻塞流程）。
- ❌ 不接 Codex。
- ❌ 不做跨平台——Windows 优先，macOS/Linux 后说。

---

## 5. 派工给开发 agent 的模板

> 「按 `I:/personal-agent/docs/派工-Round2-桌面壳与Live2D集成.md` 实现 T<编号>。
> 工程是 Rust workspace + Tauri v2，前端 React/TypeScript，后端 `conductor-core` 已存在。
> 严格遵守『不做什么』清单。完成后请：
> 1. 列出新增/修改文件 + diff 概要。
> 2. 给出该任务对应的『验收』步骤命令，方便我手工跑。
> 3. 列出本次新增的依赖（Cargo / npm）及理由。
> 4. 不要顺手扩展到下一个 T。」

---

## 6. 时间预算（粗估）

| 任务 | 预估 |
|------|------|
| T1 Tauri 骨架 | 0.5–1 天 |
| T2 SQLite 切换 + 迁移 | 1.5 天 |
| T3 Tauri commands | 0.5 天 |
| T4 TaskPanel 窗口 | 1.5 天 |
| T5 PetWindow + Live2D 渲染 | 1.5 天 |
| T6 点击穿透 | 0.5 天 |
| T7 后台 worker + 状态机 | 1 天 |
| T8 托盘 | 0.5 天 |
| T9 系统通知 | 0.5 天 |
| T10 总演练 + 打包 | 1 天 |
| **合计** | **~9 个工作日** |

预计 2 周完成（含调试与卡点）。

---

## 7. 风险与对策

| 风险 | 描述 | 对策 |
|------|------|------|
| Tauri v2 API 仍在演进 | 文档版本错位 | 钉死 Tauri 版本，遇 API 改动以 docs.rs/官网为准 |
| pixi-live2d-display + Tauri 透明窗口 | 透明背景下抗锯齿黑边 | T5 用 `backgroundAlpha: 0` + `premultipliedAlpha: false` |
| SQLite migration 失败把旧数据丢了 | 迁移工具 bug | T2 强制把 `tasks.json` 改名为 `tasks.json.bak` 而非删除；导入失败时回滚不删 bak |
| 点击穿透在多显示器/缩放比下错位 | hwnd 坐标偏移 | T6 矩形先用窗口本地坐标，多显示器问题留 Round 2.5 |
| Live2D 帧率高 CPU 占用 | PIXI 渲染压满 | T5 限 FPS 30，并在 quiet 状态停止 ticker |
| 系统通知图标显示为 Tauri 默认 | 开发态无所谓，发布需要 | T10 打包前换图标 |

---

## 8. 配套文档

- `PRD.md` §4 / §6 优先级表。
- `技术架构.md` §1–§6。
- `技术栈选型.md` §3–§8。
- `Live2D-个人推进清单.md` 车道 A（本轮依赖 A1–A5 完成）。
- `派工-Round1-MVP管道.md`（本轮的基础）。

---

*Owner: 我自己 · Last updated: 2026-05-18*
