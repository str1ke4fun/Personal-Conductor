# 派工 · Round 5 · 清和系统三文档融合设计落地

> **状态**: 🟡 融合 P0 主体收尾（基于 2026-06-05 工作区现状反推）
> **基础文档**: [清和系统三文档融合设计-20260605.md](../清和系统三文档融合设计-20260605.md)
> **派工原则**: 不补已落的、只补缺的；前端接线优先于新建；后端已通但前端未承载的优先补全
>
> **预算**: 6 个工作日 P0 收尾 + 6 个工作日 P1

---

## 〇、现状反推（避免重复派工）

> 复核人：AgentTeam · 2026-06-05
> 复核口径：与文档 §0 计划时相比，工作区已超额完成大部分 P0 清理与后端断点

### 已完成（不要再做）

| 类别 | 交付物 | 证据 |
|---|---|---|
| **Live2D 清理** | 6 项删除全部到位 | `apps/desktop/src/live2d/` 不存在；`cursor.rs` 不存在；`package.json` 无 pixi.js；`main.rs` 无 `spawn_cursor_watcher` |
| **设计 token 收口** | `tokens.css` 全套统一 | [tokens.css](../../apps/desktop/src/styles/tokens.css) 状态色/字体/scene tint 一处定义 |
| **pet.css 动效系统** | `@property` + 7 个 keyframe | [pet.css](../../apps/desktop/src/styles/pet.css) 完整呼吸/眨眼/sway + reduced-motion |
| **后端 Layer 1 断点** | reindex / 迁移 / IPC / 注入 全部闭合 | [memory.rs:1950](../../crates/conductor-core/src/memory.rs) `reindex_memory_chunk`；[0002_memory_rebuild.sql](../../crates/conductor-core/migrations/0002_memory_rebuild.sql) 已建；[commands.rs:1424-1474](../../apps/desktop/src-tauri/src/commands.rs) 6 个 IPC；[prompt.rs:113](../../crates/conductor-core/src/chat/prompt.rs) `recall_for_prompt_with_context` 已接通 |
| **Layer 3 IDE 容器** | cycle / reasoning / memory / ooda 组件齐 | [CycleIndicator.tsx](../../apps/desktop/src/windows/CycleIndicator.tsx)；[ReasoningTimeline.tsx](../../apps/desktop/src/windows/ReasoningTimeline.tsx)；[MemoryPanel.tsx](../../apps/desktop/src/windows/MemoryPanel.tsx) |
| **Pet 组件骨架** | PetBody / PetBodyShell / PetScene | [PetBody.tsx](../../apps/desktop/src/windows/pet/PetBody.tsx) 等文件已存在但**未接到 PetWindow** |
| **5/8 装饰组件** | MoodAura / MoodFace / StatusBadge / ThinkingDots / SleepZ | [decorations/index.ts](../../apps/desktop/src/windows/pet/decorations/index.ts) |

### 现状缺口（按工种分组）

| 缺口 | 影响 | 优先级 |
|---|---|---|
| PetWindow 仍 415 行直渲 AvatarRenderer，未切到 PetScene | 新组件建了但没生效，桌宠没"活物感" | **P0 阻塞** |
| 缺 `usePetAnimation` / `usePetMood` / `useAffectionGlow` 三个 hook | state → CSS 变量映射断链 | **P0 阻塞** |
| 缺 3 个装饰（HeartFloat / ActionIcon / SceneOverlay） | 情感表达层不完整 | P0 |
| 缺 `SelfBubble` / `BubbleQueue` | 主动开口无前端承载 | P0 |
| 缺 `PetInteractionLayer` | 多通道手势无承载 | P1 |
| 缺 `RelationshipCard.tsx` 关系档案页 | D2 余项 | P0 |
| `initiative.rs` 无 `pet_self_bubble` emit | 前端收不到主动气泡 | P0 |
| `send_v2.rs` 摘要自动节流未验证 | 摘要链路"通但可能不勤" | P0 |
| `app.css` 旧 pet-* 样式与新 token 重复 | 视觉一致性返工风险 | P0 |
| `ChatBubble.tsx` 无"清和"署名 + 打字机 | 与文档 A §7 不符 | P1 |
| `MemoryPanel` / `MoodIndicator` / `AffectionBadge` 视觉 token 升级 | 与融合 §2 token 不一致 | P1 |
| `GraphView.tsx` 未做 Magazine-Graph 重设计 | Issue 4 未收口 | P1 |
| 缺 SceneOverlay 装饰 + sceneKind 接入 | 文档 A §8 未收口 | P1 |
| 隐私分级首次确认 | 文档 B §七 P1 未做 | P2 |

---

## 一、P0 派工（6 个工作日 · 阻塞收尾）

> 验收：桌宠"动起来 + 有反应"；IDE 容器视觉 token 一致；关系档案页可见；后端事件链全通

### 任务 5.1 · PetWindow 切换到 PetScene 渲染路径

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **依赖** | 无 |
| **文件** | [apps/desktop/src/windows/PetWindow.tsx](../../apps/desktop/src/windows/PetWindow.tsx)、[AvatarRenderer.tsx](../../apps/desktop/src/windows/AvatarRenderer.tsx) |
| **负责人** | 前端主程 |
| **阻塞** | 是（5.2 / 5.3 / 5.4 全部依赖本任务） |

**改动清单**：

1. PetWindow.tsx 第 333 行：`<AvatarRenderer visualState={visualState} />` 替换为 `<PetScene ... />` 路径
   - PetScene 接收 `imageUrl` / `mood` / `sceneKind` 三个 prop
   - imageUrl 从 `resolveImageAsset(avatarId, activityVariant, moodZone).src` 注入（保留现有 3-tier fallback 逻辑）
2. AvatarRenderer.tsx 改造为 "usePetVisualState → resolveImageAsset → 返回 src" 的纯函数 hook 或瘦身后只暴露 `<PetBody imageUrl mood />`
3. PetWindow.tsx 第 332 行 `pet-canvas-container` 内追加 `<MoodAura mood={moodZone} />`、`<MoodFace mood={moodZone} />`
4. 保留所有现有交互（拖动、菜单、通知、聊天气泡、内联输入框）— **只替换渲染层**
5. 行数目标：从 415 行降到 ~250 行（完整瘦身需 5.1 + 5.2 + 5.4 + 5.5 一起做）

**验收**：

- [ ] 桌宠 30s 观察：≥2 次呼吸 + ≥1 次眨眼 + ≥1 次 sway（DevTools Animations 面板确认）
- [ ] mood=happy 时头顶有 lime 暖光晕（`--pet-glow-happy` drop-shadow 可见）
- [ ] mood=shy / sad / quiet 时颜色/亮度对应变化
- [ ] 拖动 / 右键菜单 / 通知 / 聊天气泡 / 内联输入全部功能不变
- [ ] `prefers-reduced-motion: reduce` 时无动效

---

### 任务 5.2 · 三个 state hook 打通 state → CSS 变量

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **依赖** | 5.1 |
| **文件** | 新建 [apps/desktop/src/windows/pet/state/](../../apps/desktop/src/windows/pet/) 下三个 hook |
| **负责人** | 前端主程 |

**新建文件**：

```
apps/desktop/src/windows/pet/state/
├── usePetAnimation.ts   # 监听 petState → 设 --pet-anim-rate / animation-play-state
├── usePetMood.ts        # 监听 moodZone → 设 --pet-glow-* 变量
└── useAffectionGlow.ts  # 监听 affection 涨分 → 触发 heart-float 事件
```

**接口约定**：

```typescript
// usePetAnimation.ts
export interface PetAnimationOptions {
  petState: PetState;
  speed?: 'normal' | 'fast' | 'slow' | 'paused';
}
export function usePetAnimation(ref: React.RefObject<HTMLElement>, options: PetAnimationOptions): void;

// usePetMood.ts
export function usePetMood(ref: React.RefObject<HTMLElement>, moodZone: MoodZone | undefined): void;

// useAffectionGlow.ts
export function useAffectionGlow(delta: number, onHeartFloat?: () => void): void;
```

**变量映射**：

| 变量 | 来源 | 取值 |
|---|---|---|
| `--pet-anim-rate` | petState | `idle=1, working=1.1, update=0.95, quiet=0, new_task=1.3` |
| `--pet-glow-happy` | moodZone=happy | 启用 drop-shadow |
| `--pet-glow-shy` | moodZone=shy | 启用 drop-shadow |
| `--pet-saturation-sad` | moodZone=sad | 0.8 |
| `--pet-brightness-quiet` | moodZone=quiet | 0.88 |

**验收**：

- [ ] 三个 hook 都有单元测试（vitest）
- [ ] petState 从 idle 切到 working 时，动画速率有可观察变化
- [ ] moodZone 变化时，对应 CSS 变量在 DOM `style` 属性上更新
- [ ] 涨分 +5 以上时触发 heart-float（与 5.3 的 HeartFloat 配合）

---

### 任务 5.3 · 补 3 个装饰组件

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **依赖** | 5.1 |
| **文件** | 新建 [apps/desktop/src/windows/pet/decorations/](../../apps/desktop/src/windows/pet/decorations/) 下三组件 |
| **负责人** | 前端 |

**新建文件**：

```
apps/desktop/src/windows/pet/decorations/
├── HeartFloat.tsx    # 好感度涨分时心形粒子从底向上飘
├── ActionIcon.tsx    # 思考/写作/等待 时显示对应 icon（与 activityVariant 联动）
└── SceneOverlay.tsx  # sceneKind=morning/afternoon/... 时套 scene-tint-* gradient
```

**接口约定**：

```typescript
// HeartFloat.tsx
export interface HeartFloatProps {
  trigger: number;          // 每次 +1 触发一次粒子
  duration?: number;        // 默认 1500ms
}

// ActionIcon.tsx
export interface ActionIconProps {
  activityVariant: ActivityVariant;
  position?: 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';
}

// SceneOverlay.tsx
export interface SceneOverlayProps {
  sceneKind: 'morning' | 'afternoon' | 'evening' | 'night' | 'music' | 'work' | 'relax';
}
```

**注意事项**：

- HeartFloat 用 framer-motion 或纯 CSS keyframe 即可，不要引入新依赖
- SceneOverlay 引用 `var(--scene-tint-${kind})` 即可，无需硬编码颜色
- ActionIcon 用 emoji 或 inline SVG，参考 [decorations/MoodFace.tsx](../../apps/desktop/src/windows/pet/decorations/MoodFace.tsx) 的 emoji 风格保持一致
- 三个组件都加进 [decorations/index.ts](../../apps/desktop/src/windows/pet/decorations/index.ts) 导出

**验收**：

- [ ] 在 PetScene 的 decorations slot 中传入三个组件后能渲染
- [ ] HeartFloat 触发后 1.5s 内播放完成
- [ ] SceneOverlay sceneKind=night 时背景变深（参考 [pet.css](../../apps/desktop/src/styles/pet.css) 中的 `--scene-tint-night`）

---

### 任务 5.4 · SelfBubble + BubbleQueue 主动开口承载

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **依赖** | 5.1（共享 PetScene 布局） |
| **文件** | 新建 [apps/desktop/src/windows/pet/feed/](../../apps/desktop/src/windows/pet/) 下两组件 |
| **负责人** | 前端 |

**新建文件**：

```
apps/desktop/src/windows/pet/feed/
├── SelfBubble.tsx   # 桌宠主动开口，独立通道（区别于 ChatBubble）
└── BubbleQueue.tsx  # 优先级队列：initiative > user_reply > notification
```

**接口约定**：

```typescript
// SelfBubble.tsx
export interface SelfBubbleProps {
  message: { id: string; content: string; priority: 'low' | 'normal' | 'high' };
  onClose: () => void;
  onPause?: () => void;
  signature?: string;  // 默认"清和"
}

// BubbleQueue.tsx
export interface BubbleMessage {
  id: string;
  content: string;
  kind: 'self' | 'user' | 'notification';
  priority: 'low' | 'normal' | 'high';
  createdAt: number;
  ttl?: number;
}
export function useBubbleQueue(): {
  queue: BubbleMessage[];
  enqueue: (m: BubbleMessage) => void;
  dequeue: () => void;
  current: BubbleMessage | null;
};
```

**设计要点**：

- 视觉与 ChatBubble 区分：SelfBubble 用 Fraunces italic 署名"清和"，无 emoji 装饰
- hover 暂停 ttl 倒计时
- 队列按 priority + createdAt 排序
- PetWindow 中替换 `chatMessage` 状态为 `useBubbleQueue()`

**验收**：

- [ ] 同一时间最多 1 个气泡显示
- [ ] 优先级 high 的气泡顶替 normal
- [ ] hover 时倒计时暂停，离开后继续
- [ ] 气泡显示"清和"署名（Fraunces italic）

---

### 任务 5.5 · PetWindow 瘦身到 ~250 行（应用 5.1-5.4）

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **依赖** | 5.1, 5.2, 5.4 |
| **文件** | [PetWindow.tsx](../../apps/desktop/src/windows/PetWindow.tsx) |
| **负责人** | 前端主程 |

**改动清单**：

1. 把窗口状态/快捷键/菜单相关代码抽到 `usePetWindowController.ts` hook
2. 把内联聊天表单抽到 `PetInlineChat.tsx` 组件
3. PetWindow 主壳只保留：`<PetScene>` + `<BubbleQueue>` + `<PetMenu>` + 控制器调用
4. 行数目标：~250 行（与文档 §11 计划的 80 行仍有差距，但已大幅瘦身；80 行目标推到 P1 + 状态机收口后达成）

**验收**：

- [ ] PetWindow.tsx ≤ 260 行
- [ ] 所有现有交互功能不丢失（拖动 / 右键 / 通知 / 聊天 / 快捷键 / 缩放 / 锁定 / 安静 / 隐藏 / 退出）
- [ ] 与之前所有验收一致的 vitest 用例通过

---

### 任务 5.6 · app.css 旧 pet-* 样式与 token 化收尾

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **依赖** | 5.1 |
| **文件** | [app.css](../../apps/desktop/src/styles/app.css) |
| **负责人** | 前端 |

**改动清单**：

1. 扫描 app.css 中所有 `.pet-*` 选择器（已发现 46 处）
2. 与 [pet.css](../../apps/desktop/src/styles/pet.css) / [tokens.css](../../apps/desktop/src/styles/tokens.css) 对比，重复的迁移到 pet.css
3. 颜色硬编码（`#xxx`）替换为 `var(--*)` 引用
4. 字体引用统一为 `var(--font-display)` / `var(--font-ui)` / `var(--font-mono)`
5. 状态色统一为 `var(--state-*)`

**验收**：

- [ ] app.css 中无硬编码 hex 颜色（除 dark 模式兼容段）
- [ ] 字体引用全部用 CSS 变量
- [ ] 与 pet.css 重复的样式只保留一处
- [ ] 视觉对比截图：app.css 改动前后无回归

---

### 任务 5.7 · RelationshipCard 关系档案页

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **依赖** | 无（独立功能） |
| **文件** | 新建 [apps/desktop/src/windows/RelationshipCard.tsx](../../apps/desktop/src/windows/RelationshipCard/) |
| **负责人** | 前端 |

**功能**：

- 关系天数（首次会话至今）
- 对话次数
- 完成的任务数
- 关系升级记录（好感度阈值跨越事件）
- "我们认识 N 天" 类文案

**数据来源**：

- 关系天数：`memory_entries` 中 `category=identity` 最早一条
- 对话次数：`conversation_summaries` count
- 任务数：`tasks` 表 count
- 升级记录：`affection_events` 表

**IPC**：复用现有 `memoryGetByCategory('identity')` + `memoryGetRecentConversations` + 新增 `affectionHistory`（如没有则先用 `affection_get_history` 命令）

**验收**：

- [ ] 右 panel 新 tab "关系" 可打开
- [ ] 数据从后端真实读取（非占位）
- [ ] 升级历史按时间倒序展示

---

### 任务 5.8 · initiative.rs 加 `pet_self_bubble` 事件

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **依赖** | 5.4（前端要有 BubbleQueue 接） |
| **文件** | [crates/conductor-core/src/initiative.rs](../../crates/conductor-core/src/initiative.rs)、[apps/desktop/src-tauri/src/commands.rs](../../apps/desktop/src-tauri/src/commands.rs) |
| **负责人** | 后端 |

**改动清单**：

1. initiative.rs 增加 `pub fn emit_proactive_bubble(state: &AppState, content: String, priority: String) -> Result<()>` 函数
2. 内部通过 `state.event_tx.send(AppEvent::PetSelfBubble { id, content, priority })` 发送
3. commands.rs 监听 `PetSelfBubble` 事件并 `app_handle.emit("pet_self_bubble", payload)`
4. AppEvent 枚举增 `PetSelfBubble` 变体
5. 在原有触发 initiative 决策的逻辑里（`interaction_patterns` 接入点）调用 `emit_proactive_bubble`

**验收**：

- [ ] `cargo test -p conductor-core` 全过
- [ ] 在 initiative 触发的对话中，后端能 emit 事件
- [ ] 前端 5.4 的 BubbleQueue 能收到事件并展示

---

### 任务 5.9 · send_v2.rs 摘要自动节流验证与补全

| 字段 | 内容 |
|---|---|
| **工时** | 0.3d |
| **依赖** | 无 |
| **文件** | [crates/conductor-core/src/chat/send_v2.rs](../../crates/conductor-core/src/chat/send_v2.rs) |
| **负责人** | 后端 |

**核查清单**：

- [ ] 确认 `index_conversation_summary` 已被自动调用（而非只在测试里）
- [ ] 节流条件：每 8-12 条消息 OR 空闲 15 分钟
- [ ] 摘要内容应包含：用户意图 + 工具结果 + 最终结论
- [ ] 摘要落表后 `memory_chunks` 有对应记录

**可能需要补的代码**：

- 在 `send_v2.rs` 每次 `assistant` 消息完成后累加计数器
- 达阈值时调用 `index_conversation_summary`
- 用 tokio interval 做空闲定时

**验收**：

- [ ] 连续 10 轮对话后，memory_chunks 出现 1 条新摘要
- [ ] 空闲 15 分钟后强制触发一次
- [ ] 现有 187 个测试不回归

---

## 二、P1 派工（6 个工作日 · 视觉与体验深化）

> 接 P0 之后启动

### 任务 5.10 · 视觉 token 一致性升级（4 个组件）

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **文件** | [MemoryPanel.tsx](../../apps/desktop/src/windows/MemoryPanel.tsx)、[MoodIndicator.tsx](../../apps/desktop/src/windows/MoodIndicator.tsx)、[AffectionBadge.tsx](../../apps/desktop/src/windows/AffectionBadge.tsx)、[OodaTimeline.tsx](../../apps/desktop/src/windows/OodaTimeline.tsx) |
| **负责人** | 前端 |

**统一项**：

- 字体：`var(--font-display)` 标题、`var(--font-ui)` 正文、`var(--font-mono)` 数据
- 颜色：状态色全部用 `var(--state-*)`
- 间距：8/12/16/24 栅格
- 边框：`1px solid var(--border-hair)` hairline

---

### 任务 5.11 · PetInteractionLayer 多通道手势

| 字段 | 内容 |
|---|---|
| **工时** | 1.5d |
| **文件** | 新建 [apps/desktop/src/windows/pet/PetInteractionLayer.tsx](../../apps/desktop/src/windows/pet/) |
| **负责人** | 前端 |

**手势矩阵**：

- 单击：欢迎气泡 + heart-float
- 双击：打开 workbench
- 长按 600ms：环形菜单
- 拖动 4px 内：吸附回原位；4px 以上：触发 drag

---

### 任务 5.12 · ChatBubble 加"清和"署名 + 打字机

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **文件** | [ChatBubble.tsx](../../apps/desktop/src/windows/ChatBubble.tsx)、复用 [StreamText.tsx](../../apps/desktop/src/windows/StreamText.tsx) |
| **负责人** | 前端 |

---

### 任务 5.13 · GraphView Magazine-Graph 重设计

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **文件** | [GraphView.tsx](../../apps/desktop/src/windows/GraphView.tsx) |
| **负责人** | 前端 |

**改动**：

- 节点用 Fraunces italic 标签
- 边用 Bricolage Grotesque tabular 数值
- 状态色统一 token
- 拓扑用杂志网格而非树状

---

### 任务 5.14 · sceneKind 接入

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **文件** | 5.3 的 SceneOverlay + 后端 `scene.rs` 推送 |
| **负责人** | 全栈 |

**改动**：

- 后端 scene 检测到变化时 emit `scene_changed` 事件
- 前端 usePetVisualState 监听并把 sceneKind 注入 PetScene

---

### 任务 5.15 · PetWindow 进一步瘦到 80 行

| 字段 | 内容 |
|---|---|
| **工时** | 0.5d |
| **文件** | PetWindow.tsx |
| **负责人** | 前端主程 |

**剩余可抽**：

- 通知/聊天气泡展示 → BubbleQueue 内部消化
- 缩放菜单 → PetScaleMenu 组件
- 状态条 → PetStatusBar 组件

---

### 任务 5.16 · 隐私分级首次确认

| 字段 | 内容 |
|---|---|
| **工时** | 1.0d |
| **文件** | [MemoryPanel.tsx](../../apps/desktop/src/windows/MemoryPanel.tsx) + 后端 settings |
| **负责人** | 全栈 |

**功能**：

- 首次启动弹"隐私分级"对话框
- 三档：基础 / 偏好 / 全量
- 设置项写入 settings 表
- 写入链路按分级开关

---

## 三、P2 派工（5+ 个工作日 · 留作下轮）

- 场景化召回权重（7 × 5 × 7）
- Pattern 可视化（"你最近一周的写作模式"）
- 数据导出/导入（SQLite + state zip）
- 第二个亮色主题 + 字体切套

---

## 四、依赖图

```
5.1 PetWindow 切到 PetScene
   ├── 5.2 三个 state hook
   ├── 5.3 三个装饰
   ├── 5.4 SelfBubble + BubbleQueue
   │      └── 5.8 initiative pet_self_bubble 事件（后端）
   ├── 5.5 PetWindow 瘦身
   └── 5.6 app.css token 化
5.7 RelationshipCard（独立）
5.9 send_v2 摘要自动节流（独立）
```

---

## 五、验收总表

### P0 验收（6 个工作日末）

| 维度 | 验收 |
|---|---|
| **A. 桌宠动效** | 30s 观察：≥2 次呼吸 + ≥1 次眨眼 + ≥1 次 sway；mood 切换有光晕；quiet 状态无动效 |
| **B. 主动开口** | initiative 触发后 1.5s 内前端能看到清和署名气泡 |
| **C. 关系档案** | 右 panel 关系 tab 可见天数/对话数/任务数/升级历史 |
| **D. 视觉 token** | app.css 无硬编码 hex；字体引用统一；状态色统一 |
| **E. 后端事件链** | 摘要自动触发 + initiative 事件 emit + LLM 真"知道"历史 |
| **F. 回归** | 现有 187 个测试全部通过；既有交互不丢失 |

### P1 验收（12 个工作日末）

- 单击/双击/长按/拖动手势全部按规格响应
- ChatBubble 署名"清和" + 打字机
- GraphView 杂志网格重设计
- SceneOverlay sceneKind 接入
- 隐私分级首次确认对话框
- PetWindow 瘦到 80 行

---

## 六、风险与备选

| 风险 | 应对 |
|---|---|
| 5.1 切换渲染路径时破坏现有交互 | 先保留 AvatarRenderer 作为 fallback，加 feature flag `usePetSceneRender`；观察 1 天再全量切换 |
| 5.4 BubbleQueue 与现有 Notification 气泡冲突 | 队列内统一 kind 字段，Notification 也走队列（5.4 子任务） |
| 5.8 initiative emit 频率过高导致前端刷屏 | 后端加 30s 内同 priority 去重 |
| 5.9 摘要节流与现有 187 测试不兼容 | 摘要触发独立成可关闭的 `enable_auto_summary` feature |

---

## 七、关键文件索引

**Layer 1 后端**：
- [crates/conductor-core/src/memory.rs](../../crates/conductor-core/src/memory.rs)
- [crates/conductor-core/src/embedding.rs](../../crates/conductor-core/src/embedding.rs)
- [crates/conductor-core/src/chat/prompt.rs](../../crates/conductor-core/src/chat/prompt.rs)
- [crates/conductor-core/src/chat/send_v2.rs](../../crates/conductor-core/src/chat/send_v2.rs)
- [crates/conductor-core/src/initiative.rs](../../crates/conductor-core/src/initiative.rs)
- [crates/conductor-core/migrations/0002_memory_rebuild.sql](../../crates/conductor-core/migrations/0002_memory_rebuild.sql)

**Layer 1 IPC**：
- [apps/desktop/src-tauri/src/commands.rs](../../apps/desktop/src-tauri/src/commands.rs)
- [apps/desktop/src/ipc/invoke.ts](../../apps/desktop/src/ipc/invoke.ts)

**Layer 2 前端**：
- [apps/desktop/src/windows/PetWindow.tsx](../../apps/desktop/src/windows/PetWindow.tsx)
- [apps/desktop/src/windows/AvatarRenderer.tsx](../../apps/desktop/src/windows/AvatarRenderer.tsx)
- [apps/desktop/src/windows/ChatBubble.tsx](../../apps/desktop/src/windows/ChatBubble.tsx)
- [apps/desktop/src/windows/MoodIndicator.tsx](../../apps/desktop/src/windows/MoodIndicator.tsx)
- [apps/desktop/src/windows/AffectionBadge.tsx](../../apps/desktop/src/windows/AffectionBadge.tsx)
- [apps/desktop/src/windows/MemoryPanel.tsx](../../apps/desktop/src/windows/MemoryPanel.tsx)
- [apps/desktop/src/windows/OodaTimeline.tsx](../../apps/desktop/src/windows/OodaTimeline.tsx)
- [apps/desktop/src/windows/pet/](../../apps/desktop/src/windows/pet/) 整目录

**Layer 3 IDE**：
- [apps/desktop/src/windows/AgentLanes.tsx](../../apps/desktop/src/windows/AgentLanes.tsx)
- [apps/desktop/src/windows/CycleIndicator.tsx](../../apps/desktop/src/windows/CycleIndicator.tsx)
- [apps/desktop/src/windows/ReasoningTimeline.tsx](../../apps/desktop/src/windows/ReasoningTimeline.tsx)
- [apps/desktop/src/windows/GraphView.tsx](../../apps/desktop/src/windows/GraphView.tsx)

**设计 token**：
- [apps/desktop/src/styles/tokens.css](../../apps/desktop/src/styles/tokens.css)
- [apps/desktop/src/styles/pet.css](../../apps/desktop/src/styles/pet.css)
- [apps/desktop/src/styles/app.css](../../apps/desktop/src/styles/app.css)

---

## 八、引用文档

- [清和系统三文档融合设计-20260605.md](../清和系统三文档融合设计-20260605.md) — 父级融合设计
- [桌宠静态PNG动态化与前端综合优化方案-20260604.md](../桌宠静态PNG动态化与前端综合优化方案-20260604.md) — 文档 A
- [桌宠记忆系统持久化与感知分层设计-20260604.md](../桌宠记忆系统持久化与感知分层设计-20260604.md) — 文档 B
- [派工-Round4-对话面板与Shell工具.md](../派工-Round4-对话面板与Shell工具.md) — 上一轮派工
- [派工-REQ002-前端文字专项优化.md](../派工-REQ002-前端文字专项优化.md) — 文案专项

---

*归档日期：2026-06-05 · Round 5 · 9 个 P0 任务 + 7 个 P1 任务 + 4 个 P2 占位*
