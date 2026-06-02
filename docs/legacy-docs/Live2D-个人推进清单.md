# Live2D 个人推进清单（双车道并行）

> 这份清单是 `Live2D-资产准备.md` 的**可执行版**。每一颗 task 都是 15–30 分钟以内能闭合的小动作，你按顺序勾。
>
> **两车道并行**：
> - **车道 A**：用官方 Hiyori 模型跑通整条管道。**最短路径，1 周目标**。
> - **车道 B**：从 nanobanana AI 生图 → Krita 拆图层 → Cubism 建模 → 替换 Hiyori。**长线，3–4 周**。
>
> **A 不阻塞 B**：A 跑通就立刻开始 B；B 做完只是把 Hiyori 文件换成你的形象文件，技术管道不变。

---

## 0. 已确定的角色设定（v3 冻结）

> ⚠️ **设定已迁移**：原"知性助理型少女（衬衫+开衫）"已废弃。
> 当前角色为「**新中式旗袍少女**」，详见 → [`新中式旗袍Live2D角色设定归档.md`](./新中式旗袍Live2D角色设定归档.md)

**摘要（v3 冻结基线，2026-05-19）**：

| 维度 | 设定 |
|---|---|
| 角色定位 | 新中式国风少女，知性优雅 |
| 年龄感 | 20 岁左右 |
| 风格 | 新中式 / muted color palette / anime key visual style |
| 服装 | 深藏青无袖改良旗袍 + 立领 + 5 颗红色圆珠盘扣（red round bead frog buttons）+ 侧开叉 |
| 发型 | **棕色中长发**半扎发 + **珍珠发夹**（pearl hairpins，主立绘）；珍珠流苏发簪（pearl tassel hairpin）作为可绑定道具 |
| 配饰（刻意保留） | 珍珠耳钉 · **黑色方形智能手表**（左腕）· **双层串珠手链**（右腕，黑珠+白/红珠）· 银色细圈戒指 · 暗青色美甲 |
| 眼神 | 温和但专注的平视（默认） |
| 体态 | 标准站姿（双手自然垂放、对称、正面），适配 Live2D rigging |
| 当前 PSD | `2Dworkspace/live2d-automation/output/qipao_v3_v3/QipaoGirl.psd`（17 层，含 overlap buffer） |
| 当前工程 | `apps/desktop/src-tauri/resources/live2d/qipao/QipaoGirl.cmo3` |

**冻结这套设定。改它要重新过一遍 nanobanana 流程，代价大。**

完整 prompt（含基础立绘 / 换装 / 表情差分 / 姿态切换 / 场景）见 `新中式旗袍Live2D角色设定归档.md`。

---

## 1. 准备工作（A、B 公用，0.5 天）

### P1. 装 Cubism Editor 5 (Free)

- [ ] 打开 https://www.live2d.com/en/cubism/download/editor/
- [ ] 下 Windows 版 installer
- [ ] 安装：默认路径即可，**勾选"Install Samples"**（这是 Hiyori 的来源）
- [ ] 启动，注册一个免费 Live2D 账号（不然 Free 版 42 天后会停用）
- [ ] 验收：能打开 `Documents/Live2D Cubism 5/Samples/Resources/Hiyori/Hiyori.cmo3`，鼠标拖拽脸部，能看到她转头

### P2. 装本地精修软件

**给你拍 Krita**（免费、离线、能存 PSD、Windows 原生）。如果你死活不想装软件，用 Photopea 网页版。

- [ ] 打开 https://krita.org/en/download/ → Windows Installer (64-bit)
- [ ] 安装路径建议 `D:\Krita\`（避免 C 盘）
- [ ] 启动，新建一个 1024x1024 文档测试，能正常用就行
- [ ] 验收：菜单 `File → Export → Export As → .psd` 能出 PSD 文件

**为什么 Krita 而不是 Photoshop**：
- PS ¥130/月订阅，Krita 免费一次到位。
- Cubism 吃的是 PSD 格式，Krita 导出的 PSD 完全兼容。
- 你这次需要的功能是"抠图 + 拆图层"，Krita 100% 够用。

**网页备选 Photopea**：https://www.photopea.com/ —— UI 几乎 = PS，不用装。但每次开网页、不能离线、广告烦。

### P3. 跑通 pixi-live2d-display demo

这是车道 A 的零号 milestone，**确认你机器跑得起来 Web Live2D**。

- [ ] 装 Node.js ≥ 18（前置文档可能已经有了）
- [ ] 找一个空目录：
  ```powershell
  cd I:\
  git clone https://github.com/guansss/pixi-live2d-display-demo.git
  cd pixi-live2d-display-demo
  npm install
  npm run dev
  ```
- [ ] 浏览器打开 `http://localhost:5173`（或它告诉你的端口）
- [ ] 看到能眨眼的角色 = OK
- [ ] 验收：鼠标移动，角色的眼睛会跟着看你

> ⚠ **遇到 npm install 报错**：八成是网络问题，跑 `npm config set registry https://registry.npmmirror.com/` 切淘宝镜像，重试。

### P4. 立项目录

- [ ] 在 `I:\personal-agent\` 下创建 `assets\live2d\` 子目录
- [ ] 创建子目录 `assets\live2d\hiyori\`（车道 A 放这里）
- [ ] 创建子目录 `assets\live2d\custom\`（车道 B 最终放这里）
- [ ] 创建子目录 `assets\live2d\nanobanana-runs\`（B 的中间产物：AI 出图、PSD、note）
- [ ] 把这一层加进 `.gitignore` 暂时不进 git（贴图很大，license 也敏感）

---

# 车道 A：用 Hiyori 跑通管道（1 周目标）

> 目标：桌宠在 Tauri 桌面壳里**眨眼 + 摆头 + 切 4 个状态**。形象不重要，管道跑通就行。

## A1. 拿到 Hiyori 资源（30 min）

- [ ] 进 `Documents/Live2D Cubism 5/Samples/Resources/Hiyori/`
- [ ] 看到这些文件：
  ```
  Hiyori.cmo3            # 工程文件，Editor 用
  Hiyori.moc3            # 运行时模型（SDK 用）
  Hiyori.model3.json     # 模型描述
  Hiyori.physics3.json   # 物理摆动
  Hiyori.pose3.json      # 姿势约束
  Hiyori.4096/           # 贴图（4096x4096）
  motions/*.motion3.json # 动作
  expressions/*.exp3.json# 表情
  ```
- [ ] 整个目录**复制**到 `I:\personal-agent\assets\live2d\hiyori\`
- [ ] 验收：目录大小约 3-5MB，文件齐全

## A2. 看 Hiyori license（10 min）

- [ ] 打开 `Documents/Live2D Cubism 5/Samples/` 下的 `FreeMaterialLicense_*.pdf`
- [ ] 重点确认这两条：
  - ✅ "Personal use" 允许
  - ✅ "Within an application you develop, for personal/learning" 允许
  - ❌ "Public redistribution" 禁止（**所以仓库不要 push Hiyori 资源**）
- [ ] 在 `assets\live2d\hiyori\` 下放一个 `LICENSE-NOTE.md`，写一句"本目录文件来源 Live2D Cubism 5 Sample，Free Material License，仅本机自用，不公开分发"
- [ ] 验收：`.gitignore` 里加 `assets/live2d/hiyori/**`，`git status` 看不到 Hiyori 文件

## A3. Editor 里熟悉模型（30 min）

- [ ] 用 Cubism Editor 打开 `Hiyori.cmo3`
- [ ] 左侧 **Parameters 面板**找到这些标准参数，每个都拖一下看效果：
  - `ParamAngleX/Y/Z`（头部旋转）
  - `ParamBodyAngleX/Y/Z`（身体旋转）
  - `ParamEyeLOpen / ParamEyeROpen`（眨眼）
  - `ParamEyeBallX/Y`（眼球方向）
  - `ParamMouthOpenY`（嘴张开）
  - `ParamBrowLY / ParamBrowRY`（眉毛）
- [ ] 顶部 **Motion / Expression 面板**，双击播放预置动作
  - 找到 `idle` / `surprised` / 类似 `tap_body` 的，记下名字
- [ ] 验收：你知道 Hiyori 自带哪些 motion 和 expression（截图存到 `assets\live2d\hiyori\available-motions.md`）

## A4. 准备 4 状态映射（20 min）

把 Conductor 的 4 个状态预先想好用哪个 motion/expression：

- [ ] 新建 `I:\personal-agent\assets\live2d\state-mapping.md`，填入：

  ```markdown
  # Conductor State → Live2D 映射

  | State | Motion | Expression | 备注 |
  |---|---|---|---|
  | idle | idle (Hiyori 自带循环) | default | 缓慢呼吸 |
  | new_task | surprised 一次 | surprised | 1s 后回 idle |
  | pending_review | tap_body 偶尔 | (无) | 每 30s 触发一次 |
  | quiet | 停止 motion | sleep（找一个闭眼 exp） | 切到这个时立刻停 |
  ```

- [ ] Editor 里逐个测试这 4 个组合，确认视觉效果不别扭
- [ ] 验收：这份 mapping 写完，**Round 2 派工 T7 直接抄它实现**

## A5. pixi-live2d-display 引 Hiyori（45 min，车道 A 关键 milestone）

- [ ] 在 P3 跑通的 demo 目录里
- [ ] 把 `assets\live2d\hiyori\` 复制到 demo 的 `public/` 下
- [ ] 修改 demo 的 model 路径指向 `/hiyori/Hiyori.model3.json`
- [ ] `npm run dev`，浏览器看到 Hiyori 出现
- [ ] 打开浏览器控制台跑：
  ```js
  // 假设 model 变量在全局（按 demo 代码调整）
  model.motion('idle');
  model.expression('default');
  ```
- [ ] 验收：能从控制台手动触发 motion/expression 切换

## A6. 接进 Tauri webview（这一步在 Round 2 派工里做）

- 占位：**A 车道在这一步切到 Round 2 派工单**。
- 你跑到这里后，Round 2 派工单 T6 「Live2D 嵌入」会接管，把上面 demo 的代码搬进 Tauri 项目。
- 验收：见 Round 2 T6。

## A7. 物理摆动 + 跟踪鼠标（30 min，A 车道收尾）

- [ ] 接进 Tauri 后，加这两行：
  ```js
  // 鼠标跟踪
  app.stage.on('pointermove', (e) => model.focus(e.global.x, e.global.y));
  // 物理摆动靠 model3.json 里的 physics3.json，pixi-live2d-display 自动调
  ```
- [ ] 摇晃鼠标，头发应该有惯性
- [ ] 鼠标移动，眼睛和头部跟随
- [ ] 验收：录一段 5 秒视频，能看出"活的"，存进 `state/log/a-channel-milestone.mp4`

---

# 车道 B：你的自定义形象（3–4 周）

> 目标：用 nanobanana 出图 → 拆图层 → Cubism 建模 → 替换 Hiyori。
> **不影响 A 车道节奏**，A 跑通后随时切换 B 的产物即可。

## 阶段 B-1：AI 出图（1–3 天，反复迭代）

### B-1.1 准备 prompt 库（30 min）

- [ ] 新建 `I:\personal-agent\assets\live2d\nanobanana-runs\prompts.md`
- [ ] 把下面这套 **base prompt** 贴进去（已经针对 Live2D 优化过）：

  ```
  Soft anime style, clean lineart, low saturation pastel colors,
  knowledgeable assistant girl, 20yo, university student aesthetic,
  soft eyes, half smile, neutral expression, mouth closed,

  simple white button-up shirt with light beige knit cardigan,
  short pleated skirt OR straight trousers (you choose),
  mid-length straight hair OR low ponytail,
  optional thin-frame round glasses,
  optional pen tucked behind ear,

  front-facing portrait, full body visible from head to knee,
  arms slightly away from body (NOT touching torso),
  hands relaxed and visible,

  flat lighting, no harsh shadows,
  pure white background, no scenery,
  clear edge separation between hair / face / shirt / cardigan / skirt,
  high resolution, sharp linework,
  768x1280 portrait composition,

  --negative: side view, three-quarter view, dynamic pose,
  fancy hairstyle, complex accessories, jewelry, hat,
  busy background, gradient background, dark colors,
  multiple characters, holding objects, weapons, food,
  closed eyes, looking away, sad expression
  ```

- [ ] 验收：prompt 文件存在，能复制粘贴

### B-1.2 第一轮出图（30 min）

- [ ] 把 B-1.1 的 prompt 贴进 nanobanana，跑 4 张
- [ ] 把 4 张全保存到 `assets\live2d\nanobanana-runs\round-01\`，文件名 `01.png / 02.png / 03.png / 04.png`
- [ ] 在 `prompts.md` 里记一笔：「Round 01：服装/姿势/眼神哪几张满意/哪几张要改」
- [ ] 验收：4 张图都在硬盘上，有标注

### B-1.3 调 prompt 跑第 2–5 轮（每轮 30 min）

- [ ] 根据 Round 01 的反馈调整 prompt（**只改 1-2 个变量**，别全改）
- [ ] 跑 4 张
- [ ] 保存到 `round-02/`、`round-03/`、依次
- [ ] 每轮结束在 `prompts.md` 记笔记
- [ ] **停止条件**：某一轮 4 张里有 1 张你看着觉得"就是她"，停
- [ ] 验收：选出的那张存为 `assets\live2d\nanobanana-runs\FINAL.png`

### B-1.4 检查 FINAL 是否适合 Live2D（10 min）

逐项打勾，**不达标回 B-1.3**：

- [ ] 正面，不是侧脸 / 不是 3/4 视角
- [ ] 头发边缘清晰，能和脸分开
- [ ] 手臂和身体之间有空隙
- [ ] 表情中性，嘴闭合
- [ ] 背景纯色（白 / 浅灰，**不能有渐变和景物**）
- [ ] 全身可见（至少到膝盖）
- [ ] 没有戴 mask / 没有遮脸物
- [ ] 验收：8 条全 ✅，存档；否则改 prompt 重跑

### B-1.5 备份原图（5 min）

- [ ] 把 FINAL.png 在 OneDrive / 移动硬盘各备份一份
- [ ] 这张图后面要在 Krita 里反复改，备份原始是命根

## 阶段 B-2：抠图与图层准备（1–2 天，Krita 操作）

### B-2.1 Krita 打开 FINAL.png（10 min）

- [ ] Krita 启动
- [ ] `File → Open` → `FINAL.png`
- [ ] 右键图层 → `Convert → To Paint Layer`（确保可编辑）
- [ ] `Image → Resize Image to New Size` → 调整到 **2048x2048**（短边补白）
  - 为什么 2048：Live2D 标准贴图常用 2048 或 4096，2048 性能+质量平衡
- [ ] `File → Save As` → `FINAL.kra`（Krita 原生格式，保留所有操作历史）
- [ ] 验收：图在画布上、有保存

### B-2.2 抠掉背景（30 min）

- [ ] 选工具 **Magic Wand**（W），点白色背景
- [ ] `Select → Grow Selection → 2px`（扩选避免毛边）
- [ ] `Edit → Cut`（或 Delete）→ 背景变透明
- [ ] 头发周围的细节用 **Eraser** 工具手动清理
- [ ] 验收：图层透明、角色边缘干净（放大 200% 看毛边）

### B-2.3 复制图层做"图层副本备份"（5 min）

- [ ] 右键当前图层 → `Duplicate Layer`，重命名为 `FULL-BACKUP`，**锁定**
- [ ] 这层永远不动，下面所有操作都在副本上做
- [ ] 验收：图层面板有两层，一锁一活

### B-2.4 拆分图层（**最关键的一步**，2–4 小时）

这一步决定 Cubism 后续能不能动起来。**Live2D 标准拆法**：

需要拆出来的图层（按从下到上的顺序）：

```
1.  back_hair         （后面的头发）
2.  body              （脖子 + 身体躯干）
3.  arm_L             （左手臂，从肩膀到手）
4.  arm_R             （右手臂）
5.  leg_L             （左腿，可选，如果裙子盖住可以不拆）
6.  leg_R             （右腿）
7.  clothing_bottom   （裙子 / 裤子）
8.  clothing_top      （上衣 / 开衫）
9.  neck              （脖子可独立，方便头部旋转）
10. face_base         （脸：不含眼/眉/嘴）
11. brow_L / brow_R   （左右眉）
12. eye_L_white       （左眼眼白）
13. eye_L_iris        （左眼瞳孔）
14. eye_L_lash_up     （左眼上睫毛 / 上眼皮）
15. eye_L_lash_down   （左眼下睫毛 / 可选）
16. eye_R_*           （右眼 4 层）
17. nose              （鼻子，可选）
18. mouth             （嘴）
19. front_hair        （前面的头发）
20. accessories       （眼镜、耳后笔等）
```

- [ ] **每个图层在 Krita 里的操作**：
  1. 在 `FULL-BACKUP` 上用 **Lasso/Magic Wand** 圈出要拆的部分
  2. `Edit → Copy` → `Edit → Paste`（粘贴成新图层）
  3. 在原 `FULL-BACKUP` 副本上把这部分擦掉（如果有遮挡关系）
  4. 给新图层重命名
- [ ] **遮挡部分需要"脑补"补全**：
  - 比如裙子盖住的腿，要画出腿的形状（即使被裙子盖住，做动作时可能露出来）
  - 比如前发盖住的眼睛，要画完整的眼睛
  - Live2D 角色"动起来"时，被盖住的部分会露出来，所以每层都必须是**完整形状**
  - 用 Krita 的 **克隆工具/笔刷** 补，参考其他公开 Live2D 模型的拆法
- [ ] 验收：图层面板从下到上能看到 15-20 层，每层都是独立可隐藏的

### B-2.5 拆图层完成度自检（30 min）

- [ ] 隐藏所有图层，从下到上一层一层显示，确认顺序对
- [ ] 隐藏前发，能看到完整的眉眼
- [ ] 隐藏头发，能看到完整的脸/头型
- [ ] 隐藏衣服，能看到完整的身体
- [ ] 移动眼睛图层一点点（临时），眼眶下面有脸的颜色（不是透明）
- [ ] 验收：4 项都过，否则回 B-2.4 补全遮挡

### B-2.6 导出 PSD（10 min）

- [ ] `File → Export → Export As`
- [ ] 格式选 **Photoshop PSD**
- [ ] 文件名 `assistant-girl-v1.psd`
- [ ] 存到 `I:\personal-agent\assets\live2d\custom\source\`
- [ ] **关键勾选**：
  - ✅ "Save with layers"（保留图层）
  - ✅ "Save with layer names"（保留命名）
- [ ] 用记事本打开 PSD（不会乱，只是看大小）应该有 20-50MB
- [ ] 验收：PSD 文件存在、大小合理

## 阶段 B-3：Cubism 建模（5–10 天，最陡的学习曲线）

> 这一阶段是**纯学习** + **大量调参**。慢慢来，不要求一次性完美。
> 推荐资源：
> - 官方教程：https://docs.live2d.com/en/cubism-editor-tutorials/
> - B 站搜「Live2D Cubism 入门 完整流程」，挑一个 5-10 小时的中文教程跟做

### B-3.1 在 Cubism 打开 PSD（15 min）

- [ ] Cubism Editor 启动
- [ ] `File → New` → 选择 "Model"
- [ ] `File → Import → PSD`，选 `assistant-girl-v1.psd`
- [ ] 等导入（可能要 10-30 秒）
- [ ] 看到右侧 **Part 面板**，列出所有 PSD 图层名
- [ ] 验收：所有图层都进来、顺序正确

### B-3.2 设置 ArtMesh（每层 5–10 min，共 1–2 小时）

- [ ] 对每个 Part，左侧菜单选 **Mesh → Auto Generate**
- [ ] 复杂部位（脸、头发）用 **Manual** 模式微调
- [ ] 头发等需要摆动的部位，网格要细一些
- [ ] 验收：所有 Part 都有 mesh，编辑器没有红色警告

### B-3.3 创建 Deformer（变形器）层级（1 小时）

变形器是 Live2D 的"骨骼"——你给一组 ArtMesh 套一个 deformer，整组就能一起变形。

- [ ] 标准变形器层级：
  ```
  Root
   ├── Body Deformer        （整个身体）
   │    ├── Head Deformer    （整个头部）
   │    │    ├── Face Deformer
   │    │    │    ├── Eye_L
   │    │    │    └── Eye_R
   │    │    └── Hair Deformer
   │    └── Torso Deformer
   ```
- [ ] 在 Part 面板右键 → `Create Warp Deformer` 创建变形器
- [ ] 拖拽 Part 进对应 Deformer
- [ ] 验收：层级结构清晰，能选中"Head Deformer"一拖整个头都跟着动

### B-3.4 绑定 30 个标准参数（5–8 小时，最累的一步）

这是 Live2D 建模的**真正大头**。每个参数都要在 Editor 里拖拽 deformer / mesh，记录"参数 = 0"、"参数 = 1"、"参数 = -1"时角色的样子。

**最少要绑定的 14 个参数**（MVP 必须）：

| 参数 | 含义 | 怎么绑 |
|---|---|---|
| ParamAngleX | 头部左右转 | Head Deformer 选中，参数 -30 时往左转、+30 时往右转，记 keyframe |
| ParamAngleY | 头部上下点 | 同上，垂直方向 |
| ParamAngleZ | 头部歪 | 同上，旋转 |
| ParamBodyAngleX/Y/Z | 身体三轴 | Body Deformer 同上 |
| ParamEyeLOpen | 左眼开合 | 上眼皮图层在 0 时下移盖住眼睛 |
| ParamEyeROpen | 右眼同上 |  |
| ParamEyeBallX | 眼球左右 | 瞳孔图层左右移动 |
| ParamEyeBallY | 眼球上下 | 瞳孔上下移动 |
| ParamBrowLY/RY | 眉毛上下 | 眉毛图层上下 |
| ParamMouthOpenY | 嘴张开 | 嘴图层变形 |
| ParamMouthForm | 嘴形（笑/哭） | 嘴的形状切换 |
| ParamBreath | 呼吸 | 身体微微缩放 |

- [ ] 每个参数绑完都按 Space 预览，自己拖一下看效果对不对
- [ ] **不要追求完美**，能动就行，后面 Round 2 接进去发现别扭再回来调
- [ ] 验收：参数面板 14 个标准参数都能拖、效果正确

### B-3.5 创建 4 个 Expression（1 小时）

- [ ] 顶部菜单 `Modeling → Animation → Create Expression`
- [ ] 创建 4 个：`default` / `surprised` / `sleep` / `focused`
- [ ] 每个 expression 调整一组参数（眉毛/眼睛/嘴/眼皮的组合）
- [ ] **导出**：`File → Export → Expression File (.exp3.json)` × 4
- [ ] 验收：4 个 .exp3.json 文件在 `assets\live2d\custom\expressions\` 下

### B-3.6 创建 4 个 Motion（2-3 小时）

- [ ] 顶部菜单 `Animation Workspace`
- [ ] 创建 4 个 motion：`idle` / `surprised` / `tap_body` / `sleep_breath`
- [ ] 每个 motion 在时间轴上加 5-10 个 keyframe
- [ ] `idle`：循环呼吸 + 眨眼，5 秒一循环
- [ ] `surprised`：1 秒动作，眉毛抬高 + 眼睛睁大 + 头微后仰
- [ ] `tap_body`：3 秒，身体晃一下
- [ ] `sleep_breath`：缓慢呼吸，眼睛闭着
- [ ] 导出 `.motion3.json` × 4
- [ ] 验收：4 个动作都能在 Editor 里循环播放

### B-3.7 物理 / 物理摆动（1 小时）

- [ ] `Modeling → Physics Settings`
- [ ] 给前发、后发、衣服下摆加物理摆动
- [ ] 参数：质量 1.0、弹性 0.5、风阻 0.4（默认值即可，不调）
- [ ] 验收：拖动头部，头发有惯性

### B-3.8 导出最终运行时文件（15 min）

- [ ] `File → Export for SDK → moc3 + model3.json`
- [ ] 导出路径 `assets\live2d\custom\runtime\`
- [ ] 自动生成：
  ```
  runtime/
  ├── assistant.moc3
  ├── assistant.model3.json
  ├── assistant.physics3.json
  ├── textures/*.png       （贴图）
  ├── motions/*.motion3.json
  └── expressions/*.exp3.json
  ```
- [ ] 验收：所有文件齐全，文件大小总和 5–20MB

## 阶段 B-4：替换 Hiyori（30 min，简单）

- [ ] Tauri 项目里把 `public/hiyori/` 改名 `public/_hiyori_backup/`
- [ ] 把 `assets\live2d\custom\runtime\` 复制为 `public/assistant/`
- [ ] 修改 webview 代码里的 model 路径：`/hiyori/Hiyori.model3.json` → `/assistant/assistant.model3.json`
- [ ] `tauri dev`
- [ ] 验收：你的自定义形象出现在桌面上，眨眼、动作、状态切换都正常

---

# 验收主线：怎么算"全做完了"

**A 车道完成**（Round 2 派工同时跑通后）：
- [ ] 桌宠是 Hiyori，常驻桌面
- [ ] 4 个状态会切换
- [ ] 鼠标跟踪眼睛
- [ ] 头发有物理摆动

**B 车道完成**：
- [ ] 桌宠换成你的自定义形象
- [ ] 上面 4 项行为完全一致
- [ ] 你看着她不觉得"塑料感"或"恐怖谷"

**整个 Live2D 工程完成的主观验收**：
- [ ] 你愿意让她出现在你的桌面上 8 小时
- [ ] 看到她干活的感觉是"被陪着"而不是"被监视"

---

## 风险与替代方案

| 风险 | 触发 | 备选 |
|---|---|---|
| nanobanana 出图始终拆不开图层（边缘混 / 透视错） | 试 5 轮 prompt 都不行 | 改用 Stable Diffusion + ControlNet 强制正面，或社区找免费符合"知性少女"的 Live2D 模型改 |
| Krita 拆图层太慢，3 周还没拆完 | 实际工作量超预期 | 暂停 B 车道，A 车道（Hiyori）继续用 1-2 个月，B 找空闲时间慢做 |
| Cubism 学习曲线劝退 | B-3.4 绑参数怎么也调不顺 | 找一个 B 站 UP 主代建模（人民币 200-500 元），或继续用 Hiyori |
| Free Material License 焦虑 | 担心 Hiyori 商用问题 | 你这是自用桌宠不商用，license OK；要 100% 干净就走 B 车道自定义 |

---

## 时间预算

| 阶段 | 预估耗时 | 难度 |
|---|---|---|
| 准备工作 P1-P4 | 0.5 天 | ⭐ |
| 车道 A（A1-A7） | 0.5-1 周 | ⭐⭐ |
| 车道 B-1（AI 出图迭代） | 1-3 天 | ⭐ |
| 车道 B-2（Krita 抠拆） | 1-2 天 | ⭐⭐⭐ |
| 车道 B-3（Cubism 建模） | 5-10 天 | ⭐⭐⭐⭐⭐ |
| 车道 B-4（替换上线） | 0.5 天 | ⭐ |
| **总计** | **3-5 周** | |

---

*关联：Live2D-资产准备.md / 派工-Round2-桌面壳与Live2D集成.md*
*Owner: 我自己 · Last updated: 2026-05-18*
