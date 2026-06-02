# Live2D 桌宠 — 资产与学习准备

> 路线：**免费 + 自学**。不买商用模型、不外包美术。
>
> 目标：跑通一个**会眨眼 / 会摆头 / 能切换 4 个状态**（idle / new_task / pending_review / quiet）的桌宠，能用在 Tauri 桌面壳里。

---

## 0. 三个核心结论先放上面

1. **官方 SDK 免费**：Live2D Cubism 4 Web SDK 对个人非商用免费，开源 license 允许嵌入自用应用。商用要走付费授权，但你这是自用桌宠，不踩线。
2. **不需要自己画模型**：网上有大量**免费可改**的 Live2D 模型（Hiyori、Mao、Haru 等是 Cubism 官方 sample，明确允许学习用途）。MVP 直接拿 Hiyori 改。
3. **真正需要学的不是画画，是动作绑定**：免费模型已经带骨骼参数，你的工作是**把"任务事件"映射到"参数变化"**（比如 `new_task → ParamEyeBallY=1` 让它"抬头看你"）。

---

## 1. 软件 / SDK（全部免费）

| 名称 | 用途 | 链接 / 备注 |
|------|------|-------------|
| **Cubism Editor 5 (Free)** | 看/调模型动作；想自己微调表情用 | https://www.live2d.com/en/cubism/download/editor/ ，Free 版有功能限制但够 MVP |
| **Cubism 4 Web SDK** | 在 Tauri webview 里加载并驱动模型 | https://www.live2d.com/en/sdk/download/web/ ，TS/JS，直接 npm 引入 |
| **pixi-live2d-display** | Web SDK 的高阶封装，省 80% 集成代码 | https://github.com/guansss/pixi-live2d-display ，MIT，强烈推荐 |
| **Cubism Viewer** | 在不写代码时预览模型动作 | 跟 Editor 一起装 |
| **OBS / ScreenToGif** | 录桌宠状态切换给自己看，方便调 | 任选 |

**Tauri 端集成栈**（写进 `技术栈选型.md`）：
```
Tauri webview
  └─ React
      └─ PIXI.js
          └─ pixi-live2d-display
              └─ Cubism 4 Core (官方 .min.js)
                  └─ model3.json (模型本体)
```

---

## 2. 模型资产（先用免费的）

### 2.1 官方 Sample 模型（**MVP 首选**）

Live2D 官方放出来明确"可用于学习与个人项目"的 sample 模型，下载即用：

| 模型 | 风格 | 推荐用途 |
|------|------|----------|
| **Hiyori** | 短发女学生，表情丰富 | **首选**，动作组最全 |
| **Mao** | 猫耳少年 | 备选，眨眼/表情动作完整 |
| **Haru** | 长发少女 | 备选，配饰多 |
| **Wanko** | 小狗 | 想走"非人桌宠"路线时考虑 |

下载位置：
- Cubism Sample Models: https://www.live2d.com/en/learn/sample/
- 安装 Cubism Editor 后，资源也在本地 `Documents/Live2D Cubism 5/Samples/` 下

**License 注意**：这些 sample 是**Free Material License**，允许个人/学习/自用，**不允许商用与公开二次分发**。给自己做桌宠没问题。

### 2.2 社区免费模型

- BiliBili、Bowlroll、DeviantArt 有大量爱好者上传，但 license 五花八门，**用前必看 Readme**。
- 注意：Vtuber 模型（如 nizima 平台）多数是付费授权，免费的少。
- MVP 阶段**不建议在社区资源上花时间**，先把官方 sample 跑通。

### 2.3 自己微调（可选，进阶）

如果想"让桌宠有自己的特色"：
1. 用 Cubism Editor 打开 sample 模型。
2. **不改建模**，只改**贴图（textures/）和参数预设（physics3.json / motion3.json）**。
3. 比如：换发色（PS 改贴图）、加一个"举着 task list 牌子"的小道具（PS 加图层 + Editor 绑参数）。

这一步不是 MVP 必须，是 Phase 2 的"加养感"动作。

---

## 3. 自学路径（按时间投入排）

### 3.1 必须看（约 4 小时）

- **Cubism Web SDK 官方 Quickstart**：https://docs.live2d.com/en/cubism-sdk-tutorials/quickstart-web/
  - 跑通"加载模型 + 让它眨眼"。
- **pixi-live2d-display 文档**：README + examples 目录
  - 学会 `model.expression('happy')`、`model.motion('idle')`、`model.focus(x, y)` 这三个 API。

### 3.2 强烈推荐（约 4 小时）

- **Cubism 参数手册**：https://docs.live2d.com/en/cubism-editor-manual/standard-parameter-list/
  - 知道 `ParamAngleX / ParamEyeBallX / ParamMouthOpenY` 这些标准参数能干什么，才能把"事件 → 状态"映射写好。
- **MotionSync 演示**：让桌宠"说话时嘴动"，未来读出摘要时用。

### 3.3 可选（看兴趣）

- **Cubism Editor 建模教程**（B站搜"Live2D 入门"，5–10 小时）：只有真的想自己画模型才需要。MVP 不需要。
- **物理摆动（Physics）调参**：让头发/裙子有惯性，提升活感。Phase 2 再说。

**总时间预算**：8 小时把 SDK + 1 个模型跑起来，足够进 Phase 2 集成。

---

## 4. 状态 → 动作映射设计

桌宠 4 个状态（来自 `技术栈选型.md` §3.2）怎么落到 Live2D 上：

| 状态 | 表情 | 动作 / 参数 | 触发场景 |
|------|------|------------|----------|
| `idle` | 默认微笑 | `motion('idle')` 循环，缓慢呼吸 | 无新事件 |
| `new_task` | 惊讶 / 抬眼 | `expression('surprised')` + `ParamEyeBallY=0.8`（看向你） | 新摘要刚生成 |
| `pending_review` | 略不耐烦 / 拉袖子 | `motion('tap_body')` 偶尔触发 | 待审堆积超 30min |
| `quiet` | 闭眼 / 戴耳机 | `expression('sleep')` + 停止 motion | 专注模式 / 静默时段 |

**实现关键**：
- 状态切换由 Conductor 后台 worker 通过 Tauri `emit` 推给 webview。
- webview 收到事件后调 `model.expression(x)` / `model.motion(x)`。
- 不要在 webview 里直接读数据库，状态判断在 Rust 侧做。

---

## 5. 资产目录建议

```
apps/desktop/src-tauri/resources/live2d/
├── hiyori/
│   ├── hiyori_pro_t10.model3.json    # 模型本体
│   ├── hiyori_pro_t10.moc3
│   ├── textures/
│   │   └── texture_00.png
│   ├── motions/
│   │   ├── idle.motion3.json
│   │   ├── surprised.motion3.json
│   │   └── tap_body.motion3.json
│   ├── expressions/
│   │   ├── default.exp3.json
│   │   ├── surprised.exp3.json
│   │   └── sleep.exp3.json
│   └── physics/
│       └── hiyori.physics3.json
└── README-license.md                  # 把官方 license 文本留一份在仓库里
```

Tauri 打包时这些资源走 `tauri.conf.json` 的 `resources` 字段，运行时 webview 用 `convertFileSrc()` 加载。

---

## 6. 风险与坑

| 风险 | 描述 | 对策 |
|------|------|------|
| Cubism Core 是闭源 .min.js | 官方核心库非开源，但允许嵌入 | 跟着官方 SDK 走，别想自己实现 |
| Web SDK + Tauri 透明窗口 | 透明背景下 PIXI canvas 抗锯齿可能有黑边 | 用 `backgroundAlpha: 0` + `premultipliedAlpha: false` |
| 模型 license 误用 | 把 sample 模型放进公开仓库 | 仓库 `.gitignore` 排除 `resources/live2d/*/textures/`，README 说明本地放置 |
| 点击穿透 vs 点击桌宠 | Windows 透明窗口默认不接收点击，但又要能点开 task panel | Tauri 加 `hit-test region`：模型轮廓内接收点击，其他区域穿透 |
| 帧率掉到 30 以下时人眼会觉得"假" | 60fps 是桌宠的"活感"门槛 | Phase 2 性能验证：i5 CPU 占用 < 5% 是目标 |

---

## 7. 立即可做的下一步（不写代码也行）

1. **下载 Cubism Editor 5 Free**，打开 Hiyori 模型，鼠标拖拽看它的动作。
   - 目的：先有"我在养一只电子宠物"的感觉，再决定真的要不要做。
2. **跑一遍 pixi-live2d-display 的 demo**：clone repo → `npm install` → `npm run dev`，浏览器里看一个能眨眼的 Hiyori。
   - 大概 30 分钟，是"我能做出来"的最小确认。
3. **想想要不要换形象**：Hiyori 是少女风，如果你更想要一个"奇怪小动物"或"机器人助手"，趁早决定，影响后续找资源。

---

## 8. 关联文档

- `PRD.md` §3.3 远期愿景 / §6 优先级表
- `技术栈选型.md` §3.2 pet-window、§8 实施 Phase 2
- `感知层调研.md` §5 桌宠形态

---

*Owner: 我自己 · Last updated: 2026-05-18*
