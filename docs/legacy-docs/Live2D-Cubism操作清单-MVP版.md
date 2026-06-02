# Cubism Editor 5.2 操作清单 — MVP 首版

> **目标**：4-6 小时内，把 `QipaoGirl.psd` 在 Cubism Editor 5.2 里建模 → 绑定 MVP 6 个核心参数 → 导出 runtime → 嫁接 Hiyori motion → Tauri 桌宠里跑起来。
>
> **MVP 范围**：眼神跟随鼠标 + 头部转动 + 简单呼吸 + 头发摆动。**不做**：完整表情、复杂手部动作、所有 30+ 标准参数。
>
> **前置已就绪**：
> - ✅ Cubism Editor 5.2 已安装
> - ✅ PSD 文件：`I:\personal-agent\2Dworkspace\live2d-automation\output\qipao_v3_v3\QipaoGirl.psd`（17 层）
> - ✅ 嫁接脚本占位：`apps/desktop/src-tauri/resources/live2d/qipao/transplant_hiyori_assets.py`
> - ✅ Hiyori 参考：`apps/desktop/src-tauri/resources/live2d/hiyori/hiyori_pro_en/`
>
> **建议节奏**：分 3 个晚上，每晚 1.5-2h，不要一次性堆完。每完成一节就保存 `.cmo3`（`Ctrl+S`）。

---

## 时间预算

| 阶段 | 预估耗时 | 难度 | 必做？ |
|---|---|---|---|
| 1. 导入 PSD + 项目设置 | 15-30 min | ⭐ | 必做 |
| 2. 生成 ArtMesh（17 个 part） | 45-60 min | ⭐⭐ | 必做（半身可跳腿） |
| 3. 创建 Deformer 层级 | 30 min | ⭐⭐ | 必做（最小 2 层） |
| 4. 绑定 MVP 6 个核心参数 | **90-120 min** | ⭐⭐⭐ | 必做 |
| 5. Physics 物理摆动（头发） | 30-45 min | ⭐⭐ | 推荐做 |
| 6. Export for SDK | 10 min | ⭐ | 必做 |
| 7. 嫁接 Hiyori motion | 15 min | ⭐ | 必做 |
| 8. Tauri 接入验收 | 15 min | ⭐ | 必做 |
| **总计** | **4-6 小时** | | |

---

## 第 1 步：导入 PSD 并设置项目（15-30 min）

### 1.1 启动 Cubism Editor 5.2 并新建项目

- [ ] 双击桌面或开始菜单的 **Live2D Cubism Editor 5** 图标
- [ ] 选择 **Modeler**（不是 Animator）
- [ ] 顶部菜单 `File` → `New`，选择 **Model**
- [ ] 弹出新建对话框，**Resolution** 选 `2048 x 2048`（默认即可），点 OK

> ⚠️ 如果 Editor 启动时要求登录，用你装 Editor 时注册的 Live2D 账号登录。Free 版账号每 42 天要登录一次。

### 1.2 导入 PSD

- [ ] 菜单 `File` → `Import` → `PSD`
- [ ] 选择文件：`I:\personal-agent\2Dworkspace\live2d-automation\output\qipao_v3_v3\QipaoGirl.psd`
- [ ] 弹出 PSD Import 对话框：
  - **Generate Mesh on Import**：✅ 勾选（自动生成网格，省力）
  - **Mesh Generation Mode**：选 **Standard**（不要 High Quality，慢且 MVP 用不上）
  - **Preserve Layer Hierarchy**：✅ 勾选
- [ ] 点 `OK`，等待 10-30 秒导入

### 1.3 导入后检查

- [ ] 左侧 **Parts 面板**应该出现 17 个 part（角色出现在画布中央）
- [ ] 顶部 **Inspector** 应该显示 Canvas size = PSD 的原始尺寸
- [ ] 鼠标滚轮缩放画布，按住中键拖动平移画布，确认能正常浏览

### 1.4 保存为 .cmo3 工程文件

- [ ] `Ctrl+S` 保存
- [ ] 文件名：`QipaoGirl.cmo3`
- [ ] 保存位置：覆盖现有的 `apps/desktop/src-tauri/resources/live2d/qipao/QipaoGirl.cmo3`
  - 或先存到 `I:\Cubism-WIP\QipaoGirl.cmo3` 等你完成所有步骤后再复制覆盖

### ✅ 第 1 步验收
- [ ] Parts 面板看到 17 个图层名（head, face_base, left_eye, ...）
- [ ] 画布上看到角色完整、无错位
- [ ] `QipaoGirl.cmo3` 文件已保存

### 🐛 排查
- **PSD 导入后角色错位**：v3 PSD 已修复 overlap，应该不会。如果出现，回头看 `output/qipao_v3_v3/QipaoGirl_preview.png` 验证 PSD 本身是否正常
- **图层数量 < 17**：检查 PSD Import 对话框的 "Preserve Layer Hierarchy" 是否勾选
- **画布看不到角色**：`View → Reset View` 或按 `Ctrl+0`

---

## 第 2 步：生成 ArtMesh（45-60 min）

> **ArtMesh 是什么**：Live2D 给每个 part（PNG 图层）生成的"可变形网格"。网格点越多，形变越细腻，但性能越差。
>
> **MVP 策略**：用 **Auto Generate**（自动）省时间。只对脸部、头发用 **Manual** 微调。腿部可跳过。

### 2.1 切到 Mesh 编辑模式

- [ ] 顶部工具栏点击 **Mesh** 按钮（看起来像一张网格图标）
- [ ] 或菜单 `Modeling` → `Mesh Edit Mode`

### 2.2 对每个 part 自动生成网格

按以下顺序，每个 part 都做：

1. 在 **Parts 面板**选中 part
2. 菜单 `Modeling` → `Edit Mesh` → `Auto Generate Mesh`
3. 弹出对话框：
   - **Density**：`Standard`
   - **Outline Margin**：`5px`（默认）
4. 点 `OK`，网格自动生成

**MVP 必做的 part 清单**（按建议顺序）：

| 序号 | Part | 模式 | 说明 |
|---|---|---|---|
| 1 | hair_back | Auto Standard | 后发，密度标准即可 |
| 2 | torso | Auto Standard | 躯干，不会大幅形变 |
| 3 | left_arm | Auto Standard | 手臂，MVP 不绑参数 |
| 4 | right_arm | Auto Standard | 同上 |
| 5 | left_leg | Auto Standard（可跳）| 半身桌宠用不上 |
| 6 | right_leg | Auto Standard（可跳）| 同上 |
| 7 | head | **Auto High Density** | 头部要细 |
| 8 | face_base | **Auto High Density** | 脸是核心 |
| 9 | left_eye | **Manual** | 关键部位，见 2.3 |
| 10 | right_eye | **Manual** | 同上 |
| 11 | left_eyebrow | Auto Standard | 太小，自动够用 |
| 12 | right_eyebrow | Auto Standard | 同上 |
| 13 | nose | Auto Standard | |
| 14 | mouth | **Manual** | 嘴要细，见 2.3 |
| 15 | hair_front | **Auto High Density** | 前发要摆动 |
| 16 | hair_side_left | **Auto High Density** | 侧发要摆动 |
| 17 | hair_side_right | **Auto High Density** | 侧发要摆动 |

> ⚠️ "Auto High Density" 在菜单里是：`Auto Generate Mesh` 对话框中 **Density** 选 `High` 而不是 `Standard`

### 2.3 Manual 模式微调（仅 3 个 part：left_eye / right_eye / mouth）

每个 part 按以下步骤：

- [ ] 选中 part
- [ ] 菜单 `Modeling` → `Edit Mesh` → `Manual` (或快捷键 `Shift+M`)
- [ ] 看到网格点（绿色）和连线（白色），可以手动加点删点
- [ ] **眼睛**：在眼眶上下边缘**手动加一圈点**（让眨眼形变更自然），点击空白处加点
- [ ] **嘴**：在嘴唇上下边缘加 5-8 个点
- [ ] 退出 Manual 模式：`Esc` 或再次点 Manual 按钮

> 💡 MVP 阶段不需要 Manual 调到完美，能用就行。后续若发现眨眼难看回来加点即可。

### 2.4 退出 Mesh 编辑模式

- [ ] 顶部工具栏点回 **Select** 按钮（箭头图标）
- [ ] 或按 `Esc` 多次

### 2.5 保存

- [ ] `Ctrl+S`

### ✅ 第 2 步验收
- [ ] 选中任一 part，画布上能看到绿色网格
- [ ] 头、脸、头发、眼、嘴的网格密度明显高于躯干
- [ ] 编辑器没有红色错误提示

### 🐛 排查
- **Auto Generate 失败 / 网格异常**：该 part 的 PNG 可能有问题。先用 Photopea 打开 PNG 检查 alpha 通道
- **Manual 模式加点没反应**：确认在 Mesh Edit Mode 下，且选中了对应 part

---

## 第 3 步：创建 Deformer 层级（30 min）

> **Deformer 是什么**：变形器，相当于"骨骼"。给一组 ArtMesh 套一个 Deformer，整组就能一起平移/旋转/缩放。
>
> **MVP 最小层级**（够用）：
> ```
> Root
>  ├── BodyDeformer        （包整个身体）
>  │    ├── HeadDeformer    （包整个头部）
>  │    │    ├── FaceDeformer
>  │    │    │    ├── EyeL_Deformer
>  │    │    │    └── EyeR_Deformer
>  │    │    └── HairDeformer
>  │    └── TorsoDeformer
> ```

### 3.1 创建 BodyDeformer（最外层）

- [ ] 在 Parts 面板**全选** torso / left_arm / right_arm / left_leg / right_leg / head / face_base / 所有 hair / 所有面部细节（即全部 17 个 part，可用 `Ctrl+A` 全选）
- [ ] 菜单 `Modeling` → `Create Warp Deformer`
- [ ] 弹出对话框：
  - **Name**：`BodyDeformer`
  - **Bezier Divisions**：`2 × 2`（默认即可，MVP 够用）
  - **Conversion**：`Apply to Selected Parts`
- [ ] 点 `OK`

### 3.2 创建 HeadDeformer（包头部相关）

- [ ] 在 Parts 面板选中：head、face_base、left_eye、right_eye、left_eyebrow、right_eyebrow、nose、mouth、hair_front、hair_side_left、hair_side_right、hair_back
- [ ] 菜单 `Modeling` → `Create Warp Deformer`
- [ ] Name = `HeadDeformer`，Bezier Divisions = `2 × 2`，点 OK
- [ ] **重要**：在 **Deformer 面板**（通常在右侧）把 HeadDeformer 拖进 BodyDeformer 下面

### 3.3 创建 FaceDeformer（包面部细节）

- [ ] 选中：face_base、left_eye、right_eye、left_eyebrow、right_eyebrow、nose、mouth
- [ ] `Modeling` → `Create Warp Deformer`
- [ ] Name = `FaceDeformer`，2×2，OK
- [ ] 在 Deformer 面板把 FaceDeformer 拖进 HeadDeformer 下面

### 3.4 创建 HairDeformer（包所有头发）

- [ ] 选中：hair_front、hair_side_left、hair_side_right、hair_back
- [ ] `Modeling` → `Create Warp Deformer`
- [ ] Name = `HairDeformer`，2×2，OK
- [ ] 拖进 HeadDeformer 下面

### 3.5 创建 EyeL_Deformer 和 EyeR_Deformer

- [ ] 选中 left_eye 单个 part
- [ ] `Modeling` → `Create Warp Deformer`
- [ ] Name = `EyeL_Deformer`，**2×2**，OK
- [ ] 拖进 FaceDeformer 下面

- [ ] 选中 right_eye 单个 part
- [ ] Name = `EyeR_Deformer`，2×2，OK
- [ ] 拖进 FaceDeformer 下面

### 3.6 创建 TorsoDeformer（包躯干和手臂）

- [ ] 选中 torso、left_arm、right_arm、left_leg、right_leg
- [ ] `Modeling` → `Create Warp Deformer`
- [ ] Name = `TorsoDeformer`，2×2，OK
- [ ] 拖进 BodyDeformer 下面

### 3.7 检查 Deformer 层级

- [ ] 打开 **Deformer 面板**（菜单 `Window` → `Deformer`，如果没显示）
- [ ] 应该看到类似：
  ```
  ▾ BodyDeformer
     ▾ HeadDeformer
        ▾ FaceDeformer
           ├ EyeL_Deformer (left_eye)
           ├ EyeR_Deformer (right_eye)
           ├ left_eyebrow
           ├ right_eyebrow
           ├ nose
           └ mouth
        ▾ HairDeformer
           ├ hair_front
           ├ hair_side_left
           ├ hair_side_right
           └ hair_back
        └ head
        └ face_base
     ▾ TorsoDeformer
        ├ torso
        ├ left_arm
        ├ right_arm
        ├ left_leg
        └ right_leg
  ```
- [ ] 选中 HeadDeformer，拖动它的控制点（Deformer 框上的紫色顶点），**整个头部应该跟着变形**

### 3.8 保存

- [ ] `Ctrl+S`

### ✅ 第 3 步验收
- [ ] Deformer 面板看到层级结构，HeadDeformer 在 BodyDeformer 下，FaceDeformer 在 HeadDeformer 下
- [ ] 拖动 HeadDeformer 顶点，整个头部跟着动
- [ ] 拖动 FaceDeformer，脸部 5 官跟着动
- [ ] `Ctrl+Z` 撤销变形回到原位

### 🐛 排查
- **拖动 Deformer 顶点没反应**：在 Parts 面板/Deformer 面板里点选了 Deformer 吗？画布上要先选中
- **Deformer 拖错位置**：右键 Deformer → `Remove from Parent`，再拖到对的位置

---

## 第 4 步：绑定 MVP 6 个核心参数（90-120 min，最关键）

> **MVP 6 个参数**：让桌宠"活起来"的最小集
> - `ParamAngleX`（头部左右转）
> - `ParamAngleY`（头部上下点）
> - `ParamAngleZ`（头部歪）
> - `ParamEyeBallX`（眼球左右）
> - `ParamEyeBallY`（眼球上下）
> - `ParamBreath`（呼吸）
>
> 每个参数的绑定流程都是：**1. 选参数 → 2. 在画布上调到 -value 状态 → 3. 在参数滑条上点击对应位置 add keyframe → 4. 切回 0 → 5. 调到 +value 状态 → 6. add keyframe → 7. 切回 0 测试中间过渡。**

### 4.1 打开 Parameters 面板

- [ ] 菜单 `Window` → `Parameter`，确保参数面板可见
- [ ] 应该看到一长串预定义参数：`ParamAngleX`、`ParamAngleY`、`ParamAngleZ`、`ParamEyeBallX`、`ParamEyeBallY`、`ParamBreath`、`ParamMouthOpenY` 等
- [ ] **如果参数面板是空的**：菜单 `Modeling` → `Parameters` → `Load Standard Parameters`，自动加载 30+ 标准参数

### 4.2 绑定 ParamAngleX（头部左右转，最简单先做）

> 操作目标：让 ParamAngleX 从 -30 到 +30 时，头部左右转动 30°

#### 步骤 4.2.1 选择目标 Deformer

- [ ] 在 **Deformer 面板**点击 `HeadDeformer`（这一步关键，绑参数前必须选中要变形的对象）

#### 步骤 4.2.2 锁定第一个关键帧（参数 = 0，默认状态）

- [ ] 在 Parameters 面板找到 `ParamAngleX`，**确认滑条在 0 位置**
- [ ] 点击 ParamAngleX 旁边的 **菱形按钮**（Add Key），变绿色 = 默认值锁定

#### 步骤 4.2.3 制作 -30 关键帧（头向左转）

- [ ] 拖动 ParamAngleX 滑条到 **-30**
- [ ] 此时画布上你**看到的还是默认状态**（因为 -30 还没绑形变）
- [ ] **手动拖动 HeadDeformer 的顶点**让头部稍微向左转（约 15-20 度，太极端会失真）
  - 拖动 HeadDeformer 框的 4 个顶点，让整个框向左旋转倾斜
  - 或选中整个 HeadDeformer 后用画布上的旋转手柄
- [ ] 调整满意后，再次点 ParamAngleX 旁的菱形 **Add Key**，锁定 -30 的形变

#### 步骤 4.2.4 制作 +30 关键帧（头向右转）

- [ ] 拖动 ParamAngleX 滑条到 **+30**
- [ ] 拖动 HeadDeformer 让头部向右转（与左转对称）
- [ ] 点菱形 Add Key

#### 步骤 4.2.5 验证

- [ ] 滑动 ParamAngleX 滑条从 -30 → 0 → +30，**头部应该平滑左右转动**
- [ ] 滑回 0，头部回正

> 💡 **第一个参数最难，绑完后续就有感觉了**。预算这一步 20-30 min。

### 4.3 绑定 ParamAngleY（头部上下点头）

操作同 4.2，但是：
- 选中：`HeadDeformer`
- 参数：`ParamAngleY`
- -30 时：头部稍微下垂（向下倾斜 10-15°）
- +30 时：头部稍微仰起

预算 10-15 min。

### 4.4 绑定 ParamAngleZ（头部歪头）

- 选中：`HeadDeformer`
- 参数：`ParamAngleZ`
- -30 时：头部向左歪（左耳靠近左肩）
- +30 时：头部向右歪

预算 10-15 min。

### 4.5 绑定 ParamEyeBallX（眼球左右）

> ⚠️ 这一步要绑两个 Deformer：EyeL_Deformer 和 EyeR_Deformer，同一个参数。

#### 4.5.1 绑 EyeL_Deformer

- [ ] 选中 `EyeL_Deformer`
- [ ] `ParamEyeBallX` = 0，Add Key
- [ ] `ParamEyeBallX` = -1（注意眼球参数是 -1 到 +1，不是 -30 到 +30）
- [ ] 拖动 EyeL_Deformer **向左平移 2-3 像素**（眼球向左看）
- [ ] Add Key
- [ ] `ParamEyeBallX` = +1，向右平移 2-3 像素，Add Key

#### 4.5.2 绑 EyeR_Deformer

- [ ] 选中 `EyeR_Deformer`
- [ ] 同上三个关键帧

#### 4.5.3 验证

- [ ] 拖 ParamEyeBallX 滑条，两个瞳孔应该**同时**左右移动

预算 15-20 min。

### 4.6 绑定 ParamEyeBallY（眼球上下）

同 4.5，但是 EyeL_Deformer 和 EyeR_Deformer 上下平移而非左右。

预算 10-15 min。

### 4.7 绑定 ParamBreath（呼吸）

> **原理**：呼吸 = 整个上半身**轻微缩放**（吸气时上身略微膨胀）

- [ ] 选中 `BodyDeformer`（整个身体）
- [ ] `ParamBreath` = 0，Add Key
- [ ] `ParamBreath` = 1
- [ ] 拖动 BodyDeformer 的顶部边缘**向上 2-3 像素**（上身略微上提，模拟吸气）
- [ ] Add Key

预算 10 min。

### 4.8 保存并测试

- [ ] `Ctrl+S`
- [ ] 依次拖动 6 个参数，观察形变效果
- [ ] 全部回 0，角色回到默认姿态

### ✅ 第 4 步验收
- [ ] 6 个参数的菱形 Add Key 按钮都是绿色（有关键帧）
- [ ] 拖动每个参数，对应部位**平滑形变**，不抖动、不撕裂
- [ ] 全部回 0 后，角色恢复初始外观

### 🐛 排查
- **拖参数没效果**：忘了选中 Deformer 就 Add Key。删掉关键帧（点菱形按钮变灰），重做
- **形变撕裂 / 错位**：Deformer 顶点拖太远了。-30 时形变控制在 15-20° 内
- **眼球只动一只**：另一只忘了绑同一个参数。重新选另一个 Deformer 绑一次

---

## 第 5 步：Physics 物理摆动 - 头发（30-45 min，推荐做）

> **效果**：头部转动时，头发自动产生惯性甩动（Live2D"伪 3D"的核心视觉效果）

### 5.1 打开 Physics 配置

- [ ] 菜单 `Modeling` → `Physics & Scene Blend` → `Physics Settings`
- [ ] 弹出 Physics 编辑窗口

### 5.2 创建一组物理摆动 - 前发

- [ ] 左侧 Group 面板，点 `+ Add Group`
- [ ] Group Name：`HairFront`
- [ ] **Input**（驱动源）：选 `ParamAngleX`，Type = `Angle`，Influence = `100%`
- [ ] **Output**（输出参数）：暂时留空，下一步操作
- [ ] **Physics Model**：
  - **Pendulum length**：`5`
  - **Air resistance**：`0.4`（默认）
  - **Reactivity**：`0.5`
  - **Mass**：`1.0`

### 5.3 给前发加 Output 参数

> Physics 的本质：Input 参数（头转动）通过物理模拟驱动 Output 参数（头发节点旋转）。
>
> 我们的 v3 模型暂时**没有为头发分段建参数**（那是头发飘动 v2，需要 +2-3h）。
>
> **MVP 简化版**：让 Physics 直接驱动 hair_front 这一整片图层的旋转。

- [ ] 在 Output 列表点 `+`
- [ ] 选择参数：实际上你需要创建一个新参数 `ParamHairFrontRotate`（这是头发整片旋转的参数）

**简化路径**：MVP 阶段如果觉得 Physics 配置太复杂，可以**跳过这一步**。等模型在 Tauri 里能跑起来之后，再回来加 Physics。

> ⏸️ **建议跳过本步骤的人**：第一次跑通的人。先把整条管道跑通再回来加 Physics。

### 5.4 关闭 Physics 窗口，保存

- [ ] 关闭 Physics Settings
- [ ] `Ctrl+S`

### ✅ 第 5 步验收（可选）
- [ ] Physics 配置完成的话：拖动 ParamAngleX，头发应该有惯性
- [ ] 跳过 Physics 的话：直接进入第 6 步

---

## 第 6 步：Export for SDK（10 min）

> 这一步把 `.cmo3` 工程文件导出成运行时格式 `.moc3 + .model3.json + textures/`。

### 6.1 选择导出路径

- [ ] 菜单 `File` → `Export Settings for Runtime File`
- [ ] 在弹出窗口检查（保持默认即可）：
  - **moc3 File Version**：`5.0`（默认）
  - **Embed Texture in moc3**：`No`（默认，保持纹理独立文件）

### 6.2 执行导出

- [ ] 菜单 `File` → `Export for Runtime` → `Export moc3 file`（或 `Export embedded files for SDK`）
- [ ] 弹出保存对话框
- [ ] 选择目录：`I:\personal-agent\apps\desktop\src-tauri\resources\live2d\qipao\runtime\`（如果不存在则新建）
- [ ] 文件名：`QipaoGirl`
- [ ] 点 `Save`

### 6.3 检查导出产物

打开 `apps/desktop/src-tauri/resources/live2d/qipao/runtime/` 目录，应该看到：

```
runtime/
├── QipaoGirl.moc3                    ← 运行时模型
├── QipaoGirl.model3.json             ← 模型描述
├── QipaoGirl.cdi3.json               ← 参数显示信息（可选）
├── QipaoGirl.physics3.json           ← 物理配置（如有）
└── QipaoGirl.<width>/                ← 纹理目录
    └── texture_00.png                ← 主纹理
    └── texture_01.png（可能有多张）
```

### ✅ 第 6 步验收
- [ ] `QipaoGirl.moc3` 文件存在，大小约 100-500 KB
- [ ] `QipaoGirl.model3.json` 文件存在
- [ ] 纹理目录有 `texture_00.png`

### 🐛 排查
- **导出按钮灰色**：检查是否有未保存的修改，先 `Ctrl+S`
- **报错"Mesh count exceeds limit"**：MVP 版不应该触发，若触发就回 Manual 减少眼睛/嘴的网格点
- **报错 "Parameter X is not bound"**：忽略，MVP 没绑的参数运行时会用默认值

---

## 第 7 步：嫁接 Hiyori motion（15 min）

> 让 QipaoGirl 立刻拥有 idle / tap / surprised 等动作，不用从零建模。

### 7.1 填写嫁接脚本常量

- [ ] 编辑 `apps/desktop/src-tauri/resources/live2d/qipao/transplant_hiyori_assets.py`
- [ ] 找到顶部 TODO 区，填写：
  ```python
  MODEL_NAME = "QipaoGirl"
  RUNTIME_OUT_DIR = Path(r"I:\personal-agent\apps\desktop\src-tauri\resources\live2d\qipao\runtime")
  # HIYORI_RUNTIME_DIR 已默认填好，不用改
  # HIYORI_MODEL_NAME = "hiyori_pro_t11"（默认 pro 版，动作丰富）
  # MOTION_GROUPS_TO_COPY = {} → 留空 = 全部复制
  ```

### 7.2 运行脚本

- [ ] 打开 PowerShell，cd 到该目录：
  ```powershell
  cd I:\personal-agent\apps\desktop\src-tauri\resources\live2d\qipao
  python transplant_hiyori_assets.py
  ```
- [ ] 应该看到输出：
  ```
  嫁接 Hiyori 资源 → ...\runtime
  [1/3] 复制 motions...
    motion ← hiyori_m01.motion3.json
    ... （共 10 个）
  [2/3] 复制 expressions...
  [3/3] 重写 model3.json...
  🎉 嫁接完成！
  ```

### 7.3 检查产物

- [ ] `runtime/motions/` 目录新增了 `hiyori_m01~m10.motion3.json`
- [ ] `runtime/QipaoGirl.model3.json` 文件被重写，包含 Motions 段
- [ ] 用文本编辑器打开 `QipaoGirl.model3.json`，应该看到 `"Motions": { "Idle": [...], "Tap": [...] }`

### ✅ 第 7 步验收
- [ ] motions 目录有 10 个 motion 文件
- [ ] model3.json 包含 Motions 字段

---

## 第 8 步：Tauri 接入验收（15 min）

### 8.1 修改前端代码

- [ ] 打开 `apps/desktop/src/live2d/Live2DCanvas.tsx`
- [ ] 找到第 9 行：
  ```typescript
  const MODEL_URL = '/live2d/hiyori/hiyori_free_en/runtime/hiyori_free_t08.model3.json';
  ```
- [ ] 改为：
  ```typescript
  const MODEL_URL = '/live2d/qipao/runtime/QipaoGirl.model3.json';
  ```

### 8.2 确认资源路径

- [ ] 检查 `apps/desktop/src-tauri/tauri.conf.json` 或 vite 配置，确认 `resources/live2d/qipao/` 已经被打进 webview 可访问路径
- [ ] 如果之前 Hiyori 路径能访问，QipaoGirl 路径也应该能访问

### 8.3 启动 Tauri dev

```powershell
cd I:\personal-agent\apps\desktop
pnpm tauri dev
```

### 8.4 验收

- [ ] 桌面右下角（或你配置的位置）出现 QipaoGirl 桌宠
- [ ] **眼神跟随鼠标**：移动鼠标，瞳孔跟着看
- [ ] **呼吸动作**：上身轻微起伏
- [ ] **idle motion**：偶尔头部摆动（Hiyori 嫁接的 idle）
- [ ] 没有报错

### ✅ 第 8 步验收（也就是 MVP 验收）
- [ ] 桌宠常驻桌面
- [ ] 眼神跟随鼠标
- [ ] 呼吸 + idle 动作循环

### 🐛 排查
- **桌宠不显示**：开 webview 开发者工具（F12），看 Console 报错。常见是 model3.json 路径不对
- **角色出现但形变扭曲**：参数绑定有问题，回 Cubism Editor 检查 Deformer 顶点位置
- **眼神不动**：检查 Live2DCanvas.tsx 里的 `model.focus()` 调用、检查模型是否有 ParamEyeBallX/Y 参数
- **报错 "Cannot read property 'expression' of undefined"**：嫁接的 expression 文件路径问题。打开 model3.json 检查 Expressions 段

---

## 🎉 MVP 完成验收清单

全部勾选 = MVP 完成：

- [ ] Cubism Editor 里：6 个 MVP 参数都能在参数面板拖动产生形变
- [ ] runtime 目录：moc3 + model3.json + textures + motions 全齐
- [ ] Tauri 桌宠跑起来：QipaoGirl 出现在桌面
- [ ] 眼神能跟鼠标
- [ ] 上身有呼吸感
- [ ] idle 动作循环播放
- [ ] 录一段 10 秒视频存到 `state/log/qipao-mvp-milestone.mp4`

---

## 接下来的扩展任务（MVP 完成后再做）

按收益从高到低：

| 优先级 | 任务 | 工时 | 收益 |
|---|---|---|---|
| ⭐⭐⭐ | 嫁接 Hiyori expression（4 种表情） | 30 min | 表情切换 |
| ⭐⭐⭐ | Physics 头发摆动（v2） | 2-3h | "伪 3D" 视觉提升 |
| ⭐⭐ | 绑定 ParamBodyAngleX/Y/Z | 1h | 上身跟随 |
| ⭐⭐ | 绑定 ParamMouthOpenY / ParamMouthForm | 30 min | 嘴动 |
| ⭐⭐ | 绑定 ParamEyeLOpen/ROpen（眨眼） | 30 min | 自然眨眼 |
| ⭐ | 出图加披肩+伞，重做 PSD/建模 | 1-2 周 | 视觉丰富度 |
| ⭐ | Physics 披肩 + 裙摆 | 1h | 衣物飘动 |

---

## 故障应急

### 如果 Cubism Editor 整个崩溃
- 你的 `.cmo3` 应该有 `.cmo3.tmp` 自动备份
- 重启 Editor，`File → Open`，找最新的 `.cmo3.tmp`

### 如果导出 moc3 失败
- 检查每个 Deformer 是否至少包含 1 个 part
- 检查没有空的 Deformer
- 检查没有循环依赖（A 在 B 下，B 在 A 下）

### 如果你卡在某一步
- **不要硬扛**：保存当前 .cmo3，截图卡住的画面
- 优先检查 Hiyori 官方样例 `Documents/Live2D Cubism 5/Samples/Resources/Hiyori/Hiyori.cmo3`，看官方怎么做的
- 实在不行，先跳过该步走完整条管道，再回头补

---

*文档版本：MVP-v1 · 创建日期：2026-05-19*
*基于 Cubism Editor 5.2 · QipaoGirl.psd v3_v3*
*关联文档：`新中式旗袍Live2D角色设定归档.md` · `Live2D-个人推进清单.md`*
