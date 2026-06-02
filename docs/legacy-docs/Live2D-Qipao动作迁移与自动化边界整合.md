# Live2D Qipao 动作迁移与自动化边界整合

更新日期：2026-05-21  
关联文档：

- `docs/Live2D-个人推进清单.md`
- `docs/Live2D-Cubism操作清单-MVP版.md`
- `apps/desktop/src-tauri/resources/live2d/qipao/transplant_hiyori_assets.py`

## 1. 当前结论

QipaoGirl 迁移 Hiyori 动作的推荐路径仍然是：

1. 在 Cubism Editor 里完成 QipaoGirl 的基础建模和参数绑定。
2. 从 Cubism Editor 手动 `Export for SDK`，导出 `moc3 + model3.json + textures` 到 `qipao/runtime/`。
3. 用现有 `transplant_hiyori_assets.py` 把 Hiyori 的 `motion3.json` 嫁接到 QipaoGirl 的 `model3.json`。
4. 修改桌宠前端的 `MODEL_URL` 指向 `/live2d/qipao/runtime/QipaoGirl.model3.json`。

不能把第 1 步完全交给开源工具或脚本。Live2D 的核心工程文件 `.cmo3`、运行时 `.moc3`、网格、Deformer、参数关键形变，仍然需要 Cubism Editor 完成。官方 External API 可以辅助读写打开中模型的参数、做外部联动或录制，但不是完整建模/导出 API。

## 2. 与现有两份清单的关系

`Live2D-个人推进清单.md` 定义的是长期路线：

- 车道 A：先用 Hiyori 跑通桌宠 Live2D 管道。
- 车道 B：用 QipaoGirl 的 PSD/Cubism 工程替换 Hiyori。
- B 完成后，本质上只是换 `model3.json` 入口，桌宠技术管道不变。

`Live2D-Cubism操作清单-MVP版.md` 是 QipaoGirl 的短路径：

- 4-6 小时内完成最小 Cubism 建模。
- 绑定 6 个 MVP 参数。
- 导出 runtime。
- 嫁接 Hiyori motion。
- 在 Tauri 桌宠里验收。

本调研补齐的是自动化边界：

- 6 参数 MVP 可以跑通桌宠和基础“活起来”的效果。
- Hiyori 全量 motion 远不止 6 个参数；未绑定的参数曲线不会产生预期形变。
- 现有嫁接脚本只负责 runtime 文件层面的复制和 `model3.json` 修改，不负责 Cubism 内的参数绑定。
- 后续不建议投入时间寻找“开源 Cubism 替代品”来直接编辑 `.cmo3/.moc3`，收益很低。

## 3. 本地资产状态

当前关键资产：

| 项 | 路径 | 状态 |
|---|---|---|
| QipaoGirl PSD | `2Dworkspace/live2d-automation/output/qipao_v3_v3/QipaoGirl.psd` | 已作为 Cubism 输入 |
| QipaoGirl 工程 | `apps/desktop/src-tauri/resources/live2d/qipao/QipaoGirl.cmo3` | 已存在 |
| Qipao runtime | `apps/desktop/src-tauri/resources/live2d/qipao/runtime/` | 需要 Cubism 导出后才会完整 |
| 嫁接脚本 | `apps/desktop/src-tauri/resources/live2d/qipao/transplant_hiyori_assets.py` | 已存在，仍需填写常量 |
| Hiyori Pro runtime | `apps/desktop/src-tauri/resources/live2d/hiyori/hiyori_pro_en/runtime/` | 已存在 |
| 桌宠入口 | `apps/desktop/src/live2d/Live2DCanvas.tsx` | 当前仍指向 Hiyori |

`transplant_hiyori_assets.py` 当前需要手动填写：

```python
MODEL_NAME = "QipaoGirl"
RUNTIME_OUT_DIR = Path(
    r"I:\personal-agent\apps\desktop\src-tauri\resources\live2d\qipao\runtime"
)
```

## 4. 参数绑定分层

### 4.1 MVP 6 参数

这 6 个参数来自 `Live2D-Cubism操作清单-MVP版.md`，是第一轮必须完成的最小集：

| 参数 | 用途 |
|---|---|
| `ParamAngleX` | 头部左右转 |
| `ParamAngleY` | 头部上下点 |
| `ParamAngleZ` | 头部歪头 |
| `ParamEyeBallX` | 眼球左右 |
| `ParamEyeBallY` | 眼球上下 |
| `ParamBreath` | 呼吸 |

完成这 6 个参数后，QipaoGirl 可以支持眼神跟随、头部基础动作、呼吸。Hiyori 的部分 idle motion 会有可见效果，但涉及眨眼、身体、眉毛、嘴、手臂的曲线不会完整表现。

### 4.2 建议补到 14 参数

这组来自 `Live2D-个人推进清单.md` 的 B-3.4。它是比 MVP 更稳的第一版：

| 参数 | 用途 |
|---|---|
| `ParamAngleX/Y/Z` | 头部三轴 |
| `ParamBodyAngleX/Y/Z` | 身体三轴 |
| `ParamEyeLOpen` / `ParamEyeROpen` | 眨眼 |
| `ParamEyeBallX/Y` | 眼球 |
| `ParamBrowLY` / `ParamBrowRY` | 眉毛上下 |
| `ParamMouthOpenY` | 嘴张开 |
| `ParamMouthForm` | 嘴形 |
| `ParamBreath` | 呼吸 |

如果目标是让 Hiyori 嫁接动作看起来不僵，建议在 MVP 跑通后优先补这组。

### 4.3 Hiyori Pro motion 实际使用的参数

本地 Hiyori Pro 的 10 个 `motion3.json` 实际包含这些参数曲线：

```text
ParamAngleX
ParamAngleY
ParamAngleZ
ParamArmLA
ParamArmLB
ParamArmRA
ParamArmRB
ParamBodyAngleX
ParamBodyAngleY
ParamBodyAngleZ
ParamBreath
ParamBrowLAngle
ParamBrowLForm
ParamBrowLX
ParamBrowLY
ParamBrowRAngle
ParamBrowRForm
ParamBrowRX
ParamBrowRY
ParamCheek
ParamEyeBallX
ParamEyeBallY
ParamEyeLOpen
ParamEyeLSmile
ParamEyeROpen
ParamEyeRSmile
ParamHairAhoge
ParamHandL
ParamHandLB
ParamHandR
ParamHandRB
ParamLeg
ParamMouthForm
ParamMouthOpenY
ParamShoulder
```

这解释了为什么“直接复制 motion 文件”只能解决文件引用，不能保证动作自然。QipaoGirl 没绑定的参数，motion 曲线即使存在也不会产生对应形变。

## 5. 工具调研整合

| 工具/方案 | 能做什么 | 不能做什么 | 结论 |
|---|---|---|---|
| Cubism Editor | 建模、网格、Deformer、参数绑定、导出 runtime | 无完整命令行建模/导出流程 | 必须保留 |
| Cubism External API | 连接打开的 Editor，读写参数，辅助外部联动/录制 | 不能替代 rigging、不能完整生成 `.cmo3/.moc3`、不能替代 Export for SDK | 可做辅助，不是替代 |
| Cubism SDK / `pixi-live2d-display` | 在 Web/Tauri 里加载 `model3.json`、播放 motion、渲染模型 | 不能编辑模型工程 | 继续作为运行时 |
| `py-moc3` 等解析库 | 低层读取/检查 `.moc3` | 不适合做高层建模工具 | 最多用于检查，不作为主路线 |
| Inochi2D | 开源 2D puppet/rigging 生态 | 不兼容 Live2D `.cmo3/.moc3` 管道 | 不是本项目短期替代 |

最终判断：短期不要换工具链。继续用 Cubism Editor 做模型生产，用脚本处理 runtime 嫁接，用 `pixi-live2d-display` 在桌宠中播放。

## 6. 推荐执行计划

### 阶段 1：按 MVP 清单完成 QipaoGirl runtime

执行 `Live2D-Cubism操作清单-MVP版.md` 的第 1-6 步：

1. 导入 PSD。
2. 生成 ArtMesh。
3. 创建 Deformer 层级。
4. 绑定 MVP 6 参数。
5. Physics 可先跳过或只做最小头发摆动。
6. `Export for SDK` 到：

```text
apps/desktop/src-tauri/resources/live2d/qipao/runtime/
```

阶段验收：

- `QipaoGirl.moc3` 存在。
- `QipaoGirl.model3.json` 存在。
- texture 目录存在。
- Cubism 里 6 个 MVP 参数拖动有形变。

### 阶段 2：运行嫁接脚本

填写 `transplant_hiyori_assets.py` 顶部常量，然后运行：

```powershell
cd I:\personal-agent\apps\desktop\src-tauri\resources\live2d\qipao
python transplant_hiyori_assets.py
```

阶段验收：

- `runtime/motions/` 下出现 Hiyori motion 文件。
- `runtime/QipaoGirl.model3.json` 包含 `FileReferences.Motions`。
- `Idle`、`Tap` 等 motion group 能在运行时被调用。

### 阶段 3：Tauri 接入

修改 `apps/desktop/src/live2d/Live2DCanvas.tsx`：

```typescript
const MODEL_URL = '/live2d/qipao/runtime/QipaoGirl.model3.json';
```

阶段验收：

- 桌宠显示 QipaoGirl。
- 鼠标移动时眼球跟随。
- idle motion 可播放。
- 未绑定参数导致的动作缺失在预期范围内，不作为 MVP 阻塞项。

### 阶段 4：补参数，不重做管道

MVP 成功后，优先补：

1. `ParamEyeLOpen` / `ParamEyeROpen`：自然眨眼。
2. `ParamBodyAngleX/Y/Z`：身体跟随和 Hiyori motion 的身体曲线。
3. `ParamMouthOpenY` / `ParamMouthForm`：嘴部和表情。
4. `ParamBrow*`：眉毛表情。
5. 手臂、手、腿、肩、头发细节参数：让 Hiyori 的 tap/flick 类动作更完整。

## 7. 后续可自动化的部分

现有脚本可以后续升级，但这些升级仍发生在 runtime 层：

- 把 `MODEL_NAME`、`RUNTIME_OUT_DIR` 改成命令行参数。
- 增加 `--dry-run`，只报告将复制哪些 motion。
- 扫描 QipaoGirl 的 `cdi3.json` 或可用参数列表，输出“motion 需要但模型缺失”的参数报告。
- 允许按 motion group 筛选，例如只迁移 `Idle` 和 `Tap`。
- 自动备份改写前的 `QipaoGirl.model3.json`。
- 生成一份 `runtime/MOTION-COMPAT-REPORT.md`，记录哪些曲线会生效、哪些不会。

这些自动化可以提升效率，但不能替代 Cubism 里的建模和参数绑定。

## 8. 不建议投入的方向

- 不建议继续找“完全开源替代 Cubism Editor 的 Live2D 编辑器”。目前没有适合直接接管 `.cmo3/.moc3` 作者流程的成熟方案。
- 不建议试图用 Cubism SDK 生成或修改 `.moc3`。SDK 是运行时，不是建模工具。
- 不建议把 Hiyori 全量 motion 当作“复制即完美”。motion 文件只是参数曲线，QipaoGirl 必须有同名参数和合理绑定。
- 不建议第一轮追求 30+ 参数全绑定。先跑通 6 参数 MVP，再补 14 参数，再按效果补全。

## 9. 参考资料

- Live2D Cubism Editor External Application Integration  
  https://docs.live2d.com/en/cubism-editor-manual/external-application-integration/
- Live2D Cubism Editor External API List  
  https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api-list/
- Live2D Cubism Editor Export moc3/motion3 files  
  https://docs.live2d.com/en/cubism-editor-manual/export-moc3-motion3-files/
- Live2D file types and extensions  
  https://docs.live2d.com/en/cubism-editor-manual/file-type-and-extension/
- Live2D community: command line exporting discussion  
  https://community.live2d.com/discussion/2100/commandline-tool-for-exporting-models
- `pixi-live2d-display`  
  https://github.com/guansss/pixi-live2d-display
- Inochi Creator  
  https://github.com/Inochi2D/inochi-creator
- `py-moc3`  
  https://pypi.org/project/py-moc3/
