# 新中式旗袍 Live2D 角色设定归档

> **版本说明**：v3 冻结版（2026-05-19）。主立绘 prompt 见第七节「基础角色立绘 Prompt」，
> 当前 Live2D 工程 `apps/desktop/src-tauri/resources/live2d/qipao/QipaoGirl.cmo3` 与
> `2Dworkspace/live2d-automation/output/qipao_v3_v3/QipaoGirl.psd` 均基于此 prompt 生成。
>
> **关键术语统一（v3 冻结，所有 prompt 必须遵守）**：
> - **盘扣**：统一写作 `5 red round bead buttons on diagonal placket`（5 颗红色圆珠形态的盘扣），**禁止用 `frog buttons` / `frog closures`**，否则模型会生成布条扭花蝴蝶结。
> - **发型（主立绘）**：`half-up hairstyle with pearl hairpins`（珍珠发夹）— **是主形象冻结项**。
> - **发型（可选道具）**：`pearl tassel hairpin`（珍珠流苏发簪）是 §4 的可绑定道具，**不写进主立绘 prompt**，避免发型轮廓变化干扰 Live2D rigging。换装/差分图可叠加。
> - **手腕物品（刻意设计，必须保留）**：左腕 `black square smartwatch`，右腕 `double-layer beaded bracelet`（黑珠+白/红珠搭配）。**所有立绘 prompt 都必须包含这两项**，不能精简掉。
> - **姿态**：主立绘用「标准站姿 + 双手自然垂放」适配 Live2D rigging，原片"坐姿"作为差分姿态保留于 §9.3。

## 一、风格基线（v3 冻结）

### 整体风格
- **核心定位**：新中式国风，清冷沉静氛围感
- **气质表现**：优雅知性，温和但专注的平视眼神（默认表情）；低敛眼神为差分表情
- **视觉调性**：柔和光线，muted color palette，navy 与 red 撞色，静谧高级
- **渲染风格关键词**：`anime key visual style` · `clean line art` · `soft cel shading` · `official art style` · `highly detailed` · `8k`

### 人物形象（主立绘冻结）
- **发型**：**棕色中长发**（brown，中等明度，不偏红不偏黑），**半扎发 + 珍珠发簪**（pearl hairpins）固定，脸颊留碎发修饰脸型
- **五官**：精致东方面部特征，深色眼眸，红唇
- **体态**：**标准站姿**（双手自然垂放、双脚与肩同宽、身体正面、头部端正、对称构图），适配 Live2D rigging；身形修长。原片同款"优雅放松坐姿"作为差分姿态保留，见第八节 §9.3

---

## 二、服饰细节

### 核心单品：藏青色改良旗袍
- **颜色**：深藏青色（海军蓝）
- **款式**：无袖设计，传统立领
- **细节**：斜襟搭配 5 颗**红色圆珠盘扣**（red round bead frog buttons / 红色珠子形态而非传统蝴蝶结盘扣，撞色亮点）
- **剪裁**：修身贴合身形，裙摆侧面开叉
- **面料**：垂坠感好，哑光质感，低调高级

---

## 三、配饰设定

| 配饰 | 细节描述 | Live2D 适配 |
|------|----------|-------------|
| **珍珠耳钉** | 单颗珍珠吊坠款，简约优雅 | 固定层 |
| **黑色智能手表** | 方形表盘，黑色表带 | 固定层 |
| **双层串珠手链** | 黑珠 + 白/红珠搭配 | 固定层 |
| **银色细圈戒指** | 简约细款 | 固定层 |
| **暗青色美甲** | 偏暗的青色（dark teal / muted cyan），修剪整齐 | 固定层 |

---

## 四、可绑定道具设计（Live2D 动态扩展）

### 1. 白色透明纱质披肩
- **设计**：半透雪纺/真丝材质，边缘绣细银色丝线暗纹
- **动态效果**：肩部自然垂坠，走动时随动作轻飘
- **Live2D 适配**：独立层绑定，可做飘动物理效果

### 2. 水墨折扇
- **设计**：扇面手绘淡墨山水/梅花，黑檀木扇骨
- **动态效果**：可开合动画
- **Live2D 适配**：手持道具层，增加人物气质表现

### 3. 珍珠发簪
- **设计**：银质簪头，簪头镶嵌一颗主珍珠 + 周围细小珍珠点缀（簪本体，**不带流苏垂挂**）
- **动态效果**：固定层，无单独摆动
- **Live2D 适配**：头部固定层，与头发同步摆动即可
- **避免**：不写 `tassel` / `dangling` / `hanging pearls` —— 生图模型会把流苏画成多条珍珠项链状装饰，破坏发型轮廓

### 4. 透明油纸伞
- **设计**：半透米白色伞面，手绘淡青色竹枝，木质伞柄
- **动态效果**：可撑可收，大肢体动作适配
- **Live2D 适配**：独立道具层，雨天场景氛围感强

### 5. 翡翠平安扣吊坠
- **设计**：细红绳系于旗袍立领内侧，小巧圆形翡翠
- **动态效果**：胸前微动效果
- **Live2D 适配**：细节加分项

### 6. 织锦手包
- **设计**：深绿/酒红织锦缎小方包，绣缠枝花纹，配细银链
- **动态效果**：手部可切换持握状态
- **Live2D 适配**：可切换道具层

---

## 五、陪伴宠物：银渐层美短猫

### 身份定位
- **角色关系**：角色的专属陪伴宠物
- **性格反差**：平时活泼闹腾、好奇心强；主人在旁/工作时瞬间变得安静沉稳
- **功能定位**：Live2D 可切换互动元素 + 场景氛围催化剂

### 详细特征
| 特征维度 | 细节描述 | Prompt 写法 |
|---------|----------|------------|
| **品种毛色** | 美国短毛猫（银渐层银虎斑），银灰色渐变毛，身上虎斑纹路极浅几乎看不出来，仅在光线下隐约可见 | `silver tabby american shorthair cat, silver gradient fur with extremely subtle almost invisible tabby stripes` |
| **面部特征** | 大圆眼睛，深褐色/黑色瞳孔；粉棕色小鼻子；额头有隐约"M"虎斑纹路 | `large round dark brown eyes, pinkish-brown nose, subtle M-shaped tabby marking on forehead` |
| **标志性姿态** | 经典"农民揣"（前爪收拢压在胸前，后腿也收拢在身体下方）——在主人面前放松、专注观察的姿态 | `in classic loaf position, all paws tucked under body, perfectly rounded compact loaf shape` |
| **气质神态** | 在主人身边时：表情严肃、沉思、冷静，像在陪着一起思考；平时状态：眼睛亮晶晶，充满好奇心，好动闹腾 | `calm and observant when with owner; curious bright eyes, playful energetic when alone` |
| **尾巴** | 尾巴带有深色环纹，尾尖深灰黑色 | `tail with dark ring markings, dark grey tip` |

### Live2D 绑定姿态（4 种）

#### A. 农民揣（主人在旁 · 安静态 · 默认）
```
small silver tabby american shorthair cat,
curled up in perfect loaf position, all paws tucked under body,
sitting calmly near the character's feet or on a low table beside her,
serious contemplative expression, quiet and well-behaved,
soft fluffy fur, silver gradient with extremely subtle almost invisible tabby stripes,
anime key visual style, clean line art, soft cel shading, matches character art style
```
> **绑定方式**：地面独立层，可切换显示/隐藏。角色不动时猫咪也不动，营造"安静陪着主人"的感觉。

#### B. 趴在肩上（近距离陪伴 · 推荐）
```
small silver tabby american shorthair cat,
curled up resting on character's LEFT shoulder, body wrapped around neck,
front paws tucked, loaf position but slightly draped over shoulder,
calm expression looking forward together with the character,
soft fluffy silver fur,
anime key visual style, clean line art, soft cel shading
```
> **绑定方式**：与左肩复合绑定，随头部动作微动。桌宠场景首选，亲和力强。

#### C. 活泼站立态（主人不在 / 待机时切换）
```
small silver tabby american shorthair cat,
standing on all four paws, body slightly crouched as if about to pounce,
bright curious eyes, tail raised slightly, alert and energetic,
one paw lifted playfully, ears perked up,
silver gradient fur with extremely subtle almost invisible tabby stripes, fluffy,
anime key visual style, clean line art, soft cel shading
```
> **绑定方式**：独立动画层，可做"待机时自动切换到活泼态，鼠标悬停时切回安静农民揣"的交互逻辑。

#### D. 趴在腿上（坐姿专属 · 安静陪伴）
```
small silver tabby american shorthair cat,
curled up in loaf position on the character's lap while she is sitting,
body rounded perfectly, paws completely hidden,
eyes half closed, relaxed sleepy expression, being perfectly still and quiet,
silver gradient fur with subtle tabby stripes,
anime key visual style, clean line art, soft cel shading
```
> **绑定方式**：坐姿差分专用。与"雅室琴房"场景搭配，氛围感极强。

### 全量设定图集成方案
在 §10 全量设定图中，猫咪放在"坐姿主立绘"脚边/旁边石凳上，采用农民揣安静态，同时角落可加一个活泼小图标暗示性格反差。
**Prompt 追加段**：
```
beside her on the stone bench, a small silver tabby american shorthair cat is curled up in perfect loaf position,
paws completely tucked under body, quiet and well-behaved, matching the character's calm vibe,
silver gradient fur with extremely subtle almost invisible tabby stripes, fluffy and soft,
cat is NOT held in hands, sitting independently on bench, observing quietly
```

### 场景联动设计
| 场景 | 猫咪推荐状态 | 氛围加成 |
|-----|------------|---------|
| **湖心亭** | 蹲在石凳另一头，农民揣安静态 | 静谧陪伴感，一人一猫同看湖面 |
| **江南雨巷** | 蹲在她脚边石阶上，毛微湿，老实不动 | 雨天里的乖巧陪伴 |
| **雅室琴房** | 趴在古琴旁边（安静）/ 偷偷扒拉琴弦（活泼待机彩蛋） | 安静时专注，没人时捣乱的性格反差 |

### Live2D 实现注意
1. **分层建议**：猫咪作为完全独立层，不与角色身体部件重叠
2. **状态切换逻辑**：默认/鼠标悬停角色 → 安静农民揣态；待机 30 秒无交互 → 自动切换活泼站立态 + 偶尔晃尾巴
3. **优先级**：农民揣基础态 → 肩上猫 → 活泼站立态 → 腿上猫
4. **一致性**：猫咪的渲染风格必须与角色完全统一（同款 `clean line art, soft cel shading`）
5. **动画设计**：活泼态增加"尾巴快速摆动、耳朵微动、偶尔伸懒腰"等小动作；安静态只有极慢的眨眼

---

## 六、场景设计

### 场景 1：湖心亭（原版氛围）
- **元素**：湖边中式凉亭，阴天，水面涟漪，远处柳树，薄雾
- **调性**：静谧淡雅，灰调柔和光线
- **适配**：最接近原片氛围

### 场景 2：江南雨巷
- **元素**：古老石板巷，青石板地面湿润，白墙黑瓦，细雨，墙边翠竹
- **调性**：诗意怀旧，暖黄灯笼光
- **适配**：持伞道具最佳组合

### 场景 3：雅室琴房
- **元素**：极简中式书房，木桌古琴，白纱窗帘，自然光，盆栽兰花，水墨挂画
- **调性**：知性温暖，柔和光影
- **适配**：折扇道具最佳组合

---

## 七、Live2D 系统扩展建议

### 换装系统
- 基础款：深藏青无袖旗袍
- 换装 1：白色暗纹旗袍
- 换装 2：酒红色旗袍

### 表情差分（4 种基础）
1. 清冷平视（默认）
2. 温柔低头
3. 浅笑
4. 微蹙眉

### 姿态切换
1. 坐姿（原片同款）
2. 站姿
3. 持伞站姿

---

## 八、生成 Prompt 集合

### 基础角色立绘 Prompt（标准站姿）
```
1girl, solo, live2d character reference sheet, full body,
young beautiful chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands, delicate facial features, dark eyes, red lips, calm and elegant neutral expression,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, 5 red round bead frog buttons on diagonal placket (small red round beads, NOT traditional knotted frog buttons), bodycon fit, side slit on skirt, matte fabric with good drape,
pearl drop earrings, black square smartwatch on left wrist, double-layer beaded bracelet, thin silver ring on right hand, dark teal nail polish, dark cyan fingernails,
standing perfectly straight and upright, symmetrical posture,
both arms relaxed hanging naturally by sides,
hands in neutral relaxed position, no extra gestures,
feet shoulder-width apart, evenly balanced stance,
body facing directly forward, head upright,
new chinese style aesthetic, cool and elegant temperament, refined and intellectual vibe,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, high contrast between navy dress and red accents,
pure white background, symmetrical composition, perfectly suitable for Live2D rigging, character design sheet,
masterpiece, best quality, 8k, ultra detailed
```

### Q版角色立绘 Prompt（可爱风格，空白背景便于抠图）
```
chibi 1girl, solo, chibi character design, super deformed style,
young cute chinese girl, large head small body, big round eyes, chibi proportions (head 1/2 body height),
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
cute facial features, large dark eyes, small red lips, gentle smile,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, 5 small red round bead buttons on diagonal placket,
shortened qipao dress reaching mid-thigh, puffy skirt shape, side slit on skirt,
pearl drop earrings, tiny black square smartwatch on left wrist, double-layer beaded bracelet on right wrist,
standing pose with slightly bent knees, cute stance,
chibi anime style, cute art style, moe aesthetic,
clean line art, soft cel shading, bright colors,
**solid pure white background, no shadows, no gradients, clean cutout ready, isolated character**,
full body character design, centered composition,
masterpiece, best quality, highly detailed
```

### 场景 Prompt（空场景，用于 Live2D 角色放置）

#### 湖心亭（空场景）
```
background, empty chinese pavilion by the lake, overcast sky, soft grey lighting,
water surface with gentle ripples, distant willow trees, misty atmosphere,
modern minimalist architecture in far background, muted color palette,
peaceful and quiet vibe, anime background style, highly detailed,
no characters, suitable for live2d character placement
```

#### 江南雨巷（空场景）
```
background, old stone alley in southern china, wet bluestone ground,
white walls and black tiled roofs, subtle rain falling, bamboo by the wall,
warm dim lantern light, shallow depth of field, nostalgic atmosphere,
cool color tones, water reflection on ground, anime background style
```

#### 雅室琴房（空场景）
```
background, minimalist chinese study room, solid wood guqin on wooden table,
thin white gauze curtain by the window, soft natural light,
potted orchid, ink scroll painting on wall, matte dark wood furniture,
warm and elegant atmosphere, soft shadows, anime background style
```

---

## 九、分模块 Prompt 集合

### 9.1 换装系统 Prompt

#### 深蓝旗袍（基础款）
```
1girl, solo, full body, live2d character sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins, delicate facial features,
wearing dark navy blue sleeveless modified qipao, mandarin collar, 5 red round bead frog buttons on diagonal placket (small red round beads, NOT traditional knotted frog buttons),
side slit, matte fabric, elegant silhouette,
standing perfectly straight and upright,
both arms relaxed hanging naturally by sides,
hands in neutral position, no extra gestures,
feet shoulder-width apart, balanced posture,
body facing directly forward, head upright,
calm and elegant neutral expression,
anime key visual style, clean line art, soft cel shading,
white background, front view, symmetrical composition,
suitable for Live2D rigging,
masterpiece, best quality
```

#### 白色暗纹旗袍
```
1girl, solo, full body, live2d character sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins, delicate facial features,
wearing ivory white sleeveless modified qipao, mandarin collar, 5 silver round bead frog buttons on diagonal placket (small silver round beads, NOT traditional knotted frog buttons),
subtle dark floral jacquard texture all over fabric, side slit, silk-like sheen,
standing perfectly straight and upright,
both arms relaxed hanging naturally by sides,
hands in neutral position, no extra gestures,
feet shoulder-width apart, balanced posture,
body facing directly forward, head upright,
calm and elegant neutral expression,
anime key visual style, clean line art, soft cel shading,
white background, front view, symmetrical composition,
suitable for Live2D rigging,
masterpiece, best quality
```

#### 酒红旗袍
```
1girl, solo, full body, live2d character sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins, delicate facial features,
wearing burgundy red sleeveless modified qipao, mandarin collar, 5 gold round bead frog buttons on diagonal placket (small gold round beads, NOT traditional knotted frog buttons),
side slit, satin fabric with gentle sheen, rich and elegant color,
standing perfectly straight and upright,
both arms relaxed hanging naturally by sides,
hands in neutral position, no extra gestures,
feet shoulder-width apart, balanced posture,
body facing directly forward, head upright,
calm and elegant neutral expression,
anime key visual style, clean line art, soft cel shading,
white background, front view, symmetrical composition,
suitable for Live2D rigging,
masterpiece, best quality
```

---

### 9.2 表情差分 Prompt

#### 清冷平视（默认表情）
```
1girl, solo, close-up portrait, bust shot, live2d expression sprite,
young beautiful chinese woman, medium brown hair, face-framing strands,
dark eyes looking straight ahead, neutral calm expression, slightly serious,
red lips, delicate makeup, pearl earrings,
anime key visual style, clean line art, soft cel shading,
white background, front view,
masterpiece, best quality
```

#### 温柔低头
```
1girl, solo, close-up portrait, bust shot, live2d expression sprite,
young beautiful chinese woman, medium brown hair, face-framing strands,
eyes looking down gently, long eyelashes, soft melancholic expression,
head tilted slightly downward, red lips slightly parted,
anime key visual style, clean line art, soft cel shading,
white background, front view,
masterpiece, best quality
```

#### 浅笑
```
1girl, solo, close-up portrait, bust shot, live2d expression sprite,
young beautiful chinese woman, medium brown hair, face-framing strands,
gentle faint smile, lips curved upward slightly, eyes warm and soft,
subtle dimples, relaxed and approachable expression,
anime key visual style, clean line art, soft cel shading,
white background, front view,
masterpiece, best quality
```

#### 微蹙眉
```
1girl, solo, close-up portrait, bust shot, live2d expression sprite,
young beautiful chinese woman, medium brown hair, face-framing strands,
eyebrows slightly furrowed, thoughtful and concerned expression,
eyes slightly narrowed, lips pressed together gently,
pensive and worried look,
anime key visual style, clean line art, soft cel shading,
white background, front view,
masterpiece, best quality
```

---

### 9.3 姿态切换 Prompt

#### 坐姿（原图同款）
```
1girl, solo, full body, live2d pose sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins,
wearing dark navy qipao with 5 red round bead frog buttons (small red round beads, NOT traditional knotted frog buttons),
sitting on grey stone bench by the water, legs crossed slightly,
one hand resting on bench, other hand on knee,
body leaning back slightly, head turned to side looking away,
elegant and relaxed posture, calm expression,
anime key visual style, clean line art, soft cel shading,
white background,
masterpiece, best quality
```

#### 标准站姿（完全站直，手部自然）
```
1girl, solo, full body, live2d pose sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins,
wearing dark navy sleeveless qipao with 5 red round bead frog buttons on diagonal placket (small red round beads, NOT traditional knotted frog buttons),
standing perfectly straight, symmetrical upright posture,
both arms relaxed and hanging naturally by sides,
hands in neutral relaxed position, no extra gestures,
feet shoulder-width apart, standing evenly balanced,
body facing directly forward, head upright,
calm and elegant neutral expression,
simple white background, clean silhouette,
anime key visual style, clean line art, soft cel shading,
symmetric composition suitable for rigging,
masterpiece, best quality
```

#### 优雅站姿（带轻微姿态）
```
1girl, solo, full body, live2d pose sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins,
wearing dark navy qipao with 5 red round bead frog buttons (small red round beads, NOT traditional knotted frog buttons),
standing gracefully, body slightly turned three-quarter view,
one arm relaxed by side, other hand lightly touching hair,
weight shifted to one hip, long and slender silhouette,
confident and elegant posture, calm expression,
anime key visual style, clean line art, soft cel shading,
white background,
masterpiece, best quality
```

#### 持伞站姿
```
1girl, solo, full body, live2d pose sprite,
young beautiful chinese woman, medium brown half-up hair with pearl hairpins,
wearing dark navy qipao with 5 red round bead frog buttons (small red round beads, NOT traditional knotted frog buttons),
standing holding translucent paper umbrella above shoulder,
umbrella with light bamboo pattern painted on surface, wooden handle,
one hand holding umbrella handle, other hand relaxed by side,
body slightly tilted, graceful posture as if in light rain,
anime key visual style, clean line art, soft cel shading,
white background,
masterpiece, best quality
```

#### 披肩 + 双手持伞（左肩斜倚伞，Live2D 推荐姿态）
```
1girl, solo, full body, live2d pose sprite,
young beautiful chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands, delicate facial features, dark eyes, red lips, calm and elegant neutral expression,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, 5 red round bead frog buttons on diagonal placket (small red round beads, NOT traditional knotted frog buttons), bodycon fit, side slit on skirt, matte fabric with good drape,
draped with a translucent sheer white silk gauze shawl over both shoulders, very thin chiffon material, soft silver embroidery along the edges, gently flowing past upper arms,
holding a translucent oil-paper umbrella with BOTH hands on the LEFT side of body, umbrella handle resting against left shoulder, umbrella closed and tilted backward against shoulder (NOT opened above head), shaft pointing diagonally up-back, both hands clasped on the umbrella handle around waist level,
umbrella surface: half-transparent ivory paper with hand-painted light cyan bamboo branches, dark wooden handle and ribs,
pearl drop earrings, black square smartwatch on left wrist, double-layer beaded bracelet, thin silver ring on right hand, dark teal nail polish, dark cyan fingernails,
standing perfectly straight, body facing directly forward, head upright,
feet shoulder-width apart, evenly balanced stance,
symmetrical posture except for the umbrella tilted to one side,
calm and elegant neutral expression,
new chinese style aesthetic, cool and elegant temperament, refined and intellectual vibe,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, high contrast between navy dress and red accents,
pure white background, composition suitable for Live2D rigging, character design sheet,
masterpiece, best quality, 8k, ultra detailed
```

> **rigging 提示**：本 prompt 刻意设置「双手抓握同一伞柄、伞斜倚左肩」是
> 为了让伞与角色形成稳定的复合刚体——Live2D 里只需把整支伞作为
> "左肩附属层"绑定，避免动画时伞和手分离漂移。如果你要做"撑开的伞"
> 应另出图（伞张开后遮挡上半身大量像素，rigging 复杂度翻倍）。

#### 披肩 + 双手持伞（半身版 / 桌宠首选）
```
1girl, solo, upper body bust shot from waist up, live2d desktop pet sprite,
young beautiful chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands, delicate facial features, dark eyes, red lips, calm and elegant neutral expression,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, 5 red round bead frog buttons on diagonal placket (small red round beads, NOT traditional knotted frog buttons), bodycon fit, matte fabric with good drape,
draped with a translucent sheer white silk gauze shawl over both shoulders, very thin chiffon material, soft silver embroidery along the edges, gently flowing past upper arms,
holding a translucent oil-paper umbrella with BOTH hands on the LEFT side of body, umbrella handle resting against left shoulder, umbrella closed and tilted backward against shoulder (NOT opened above head), shaft pointing diagonally up-back, both hands clasped on the umbrella handle around chest level (visible in frame),
umbrella surface: half-transparent ivory paper with hand-painted light cyan bamboo branches, dark wooden handle and ribs,
pearl drop earrings, black square smartwatch on left wrist, double-layer beaded bracelet, thin silver ring on right hand, dark teal nail polish, dark cyan fingernails,
body facing directly forward, head upright, both hands visible in frame, frame cuts at upper waist,
calm and elegant neutral expression,
new chinese style aesthetic, cool and elegant temperament, refined and intellectual vibe,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, high contrast between navy dress and red accents,
pure white background, composition suitable for Live2D rigging, character design sheet,
masterpiece, best quality, 8k, ultra detailed
```

> **半身版 vs 全身版**：
> - **半身版** = 截到上腰，画面只保留头+上身+披肩+伞+双手。**桌宠场景首选**，因为桌宠常驻显示尺寸 240×320 实际只露上半身。出图阶段就裁掉下半身意味着 Krita 拆图层和 Cubism 建模少做 30-40% 工作。
> - **全身版** = 完整人物从头到脚。**正式立绘 / 展示场景用**。
> - **首次跑通 MVP 选半身版**；未来要展示完整角色再重出全身版。
> - 两版都用同一个伞-肩复合姿态设计，未来切换不冲突。

---

## 十、全量角色设定图 Prompt（single-shot reference sheet）

> **用途**：用一张图同时呈现角色的"全身设定 + 配饰特写 + 原片同款坐姿 + 湖心亭背景 + 主要可绑定道具"，
> 作为对外展示用「角色档案立绘」、视觉风格基准、不同人 / 不同 prompt 工具间对齐用。
> **不用于 Live2D rigging**（rigging 用第七节「标准站姿」干净版本）。

### 10.1 全量角色设定图（中央坐姿 + 道具环绕 + 配饰特写）

```
character reference sheet, full body view, single character design document, 1girl, solo,

central main pose:
young beautiful chinese woman around 20 years old, sitting elegantly on grey weathered stone bench by lakeside, legs crossed slightly at ankles, body leaning back lightly, head turned slightly to the side gazing softly into the distance, one hand resting on bench surface, other hand resting gently on her lap, refined intellectual vibe, calm and elegant neutral expression with slight melancholy,

face & hair:
delicate east asian facial features, dark almond eyes with long lashes, defined eyebrows, red lips, fair skin, medium-length brown hair (warm chestnut brown, NOT black, NOT blonde), half-up hairstyle secured with pearl hairpins on top, loose face-framing strands curling around cheeks,

outfit (core):
wearing dark navy blue sleeveless modified qipao dress (mandarin collar, bodycon fit, side slit on skirt, matte fabric with subtle silk drape, ankle-length),
5 red round bead frog buttons running diagonally across the placket from collar to right hip (small red round beads, NOT traditional knotted/butterfly frog buttons, evenly spaced, vivid red as the only warm accent),
draped with a translucent sheer white silk gauze shawl over both shoulders, very thin chiffon material with soft silver embroidery along the edges, gently flowing past upper arms,

jewelry & accessories (all visible):
single pearl drop earrings on both ears,
black square smartwatch with black silicone strap on LEFT wrist,
double-layer beaded bracelet (black beads layer + white/red beads layer) on RIGHT wrist,
thin silver ring on right hand,
dark teal / muted cyan nail polish on fingernails (clearly visible),
small jade peace-buckle pendant on thin red string tucked inside mandarin collar (slightly peeking out),

prop layout around the figure (design-sheet style, do NOT held in hand of main pose):
upper left corner: closed oil-paper umbrella with hand-painted light cyan bamboo branches and dark wooden handle (item callout),
upper right corner: open black ink folding fan with painted faint plum blossom on rice paper, ebony ribs (item callout),
lower left corner: pearl tassel hairpin (silver hairpin head with 3-5 strands of pearl tassels) (item callout),
lower right corner: small dark green brocade pouch with embroidered floral pattern and silver chain strap (item callout),
each prop neatly placed at corner with thin label-style positioning, NOT cluttered, NOT touching the main figure,

background:
empty traditional chinese pavilion by misty lake in the far background, overcast soft grey sky, gentle water ripples, distant willow trees, light fog, modern minimalist architecture barely visible in haze, muted color palette, peaceful contemplative atmosphere, soft natural overcast lighting evenly lighting subject,

color & lighting:
muted cool tone overall, dark navy as dominant color, vivid red round buttons as primary accent, dark teal nails and translucent white shawl as secondary highlights, soft overcast lighting with no harsh shadows, gentle ambient illumination, low saturation pastel atmosphere, high quality cel shading,

composition & style:
character design reference sheet style, single composition, full body centered in frame, props labeled at four corners,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style, 8k, ultra detailed,
masterpiece, best quality, character archive illustration,

NEGATIVE: multiple characters, character clones, twin views, multiple poses of same character, busy background, complex scene, dark gloomy atmosphere, watermark, text overlay, signature, logo, hands holding any prop, opened umbrella, modern setting, casual clothing, school uniform, bare feet
```

### 10.2 使用提示

- **生成尺寸**：建议 `1024×1536` 或 `1280×1920` 竖图，方便展示从头到脚 + 道具环绕
- **失败模式**：AI 经常会无视"NOT in hand"指令而把伞画到角色手里。如果出现这种情况：
  1. 把 `prop layout around the figure` 段改为 `floating decorative item icons in corners with small size, clearly separated from character by white space`
  2. 或者干脆删掉道具，只保留主体 + 配饰 + 背景 + 坐姿
- **冲突项排序**：如果生成 4 张里没有一张准确，**先保住主体 4 项**：发型 / 旗袍盘扣 / 智能手表 / 坐姿，**其次保配饰**，**最后再要求道具角落环绕**。AI 模型一次能保住的元素数量有限
- **风格对齐**：本 prompt 与第七节「基础角色立绘」共享风格关键词（`anime key visual style, clean line art, soft cel shading`），保证两张图视觉一致，可以同时用于不同场景

### 10.3 全量设定图 Prompt（中英混合强化版 · 测试模型上限用）

> 与 9.1 并列保留。本版本把中文设定说明也直接写进 prompt，强化约束、按维度结构化，
> 用于"压力测试"模型一次性能保住多少元素。模型不吃中文时，可只取每段后的 English 行。

```text
【任务类型 / Output goal】
一张图：中心"坐姿全身主立绘" + 四角"道具小图标/物品特写" + 背景"湖心亭阴天氛围"
风格：character reference sheet / character archive illustration
只画一个人：1girl, solo

【构图 / Composition】
- 竖构图 full body centered
- 中央：坐姿主立绘占画面 70-80%
- 四角：道具小图标（每个占角落 10% 以内），与主体明显留白，不接触身体
- 背景：湖心亭+湖面薄雾，低对比度，不抢主体
- 禁止：同一角色多姿势、多视图、多分身、分屏、九宫格
English: character reference sheet, full body centered, single composition, one character only, props as small corner callouts, clean separation, lots of white space.

【人物信息 / Character】
- 20岁左右，新中式国风少女，知性优雅，清冷沉静氛围
- 表情：平静、克制、略带故事感（不要夸张笑，不要哭）
English: young beautiful chinese woman around 20, refined intellectual vibe, calm elegant neutral expression, subtle melancholy.

【坐姿（中央主立绘）/ Main pose】
- 坐在灰色风化石凳上（湖心亭旁），姿态优雅放松
- 右腿轻搭在左腿上（或脚踝轻交叉），身体微微后靠
- 头部略微侧转（不超过15度），视线稍偏远处，脸仍清晰可见
- 左手：自然撑在石凳表面（手掌放松）
- 右手：自然放在大腿上
English: sitting on grey weathered stone bench by lakeside, legs crossed slightly at ankles, body leaning back lightly, head turned slightly to the side gazing softly into the distance, one hand resting on bench, other hand resting gently on her lap.

【脸与头发 / Face & Hair】
- 东方面部，深色杏眼，长睫毛，眉形清晰，红唇（不要浓艳口红）
- 棕色中长发（warm chestnut brown，不是黑发，不是金发）
- 半扎发 + 珍珠发夹固定；脸颊两侧碎发修饰脸型
- 发夹是 pearl hairpins（珍珠发夹，简洁，不要夸张头饰）
English: delicate east asian facial features, dark almond eyes, defined eyebrows, red lips, fair skin, medium-length warm chestnut brown hair, half-up hairstyle secured with pearl hairpins, loose face-framing strands.

【服装核心（必须准确）/ Outfit core (MUST)】
- 旗袍：深藏青色无袖改良旗袍（海军蓝），立领，修身，哑光质感，垂坠感好
- 盘扣：斜襟从领口到右胯一共 5 颗"红色圆珠盘扣"
  注意：是小红色圆珠（beads），不是传统扭结/蝴蝶结盘扣
English: dark navy blue sleeveless modified qipao dress, mandarin collar, bodycon fit, matte fabric with subtle silk drape, side slit; 5 red round bead frog buttons on diagonal placket from collar to right hip (small red round beads, NOT traditional knotted/butterfly frog buttons).

【配饰（必须可见）/ Accessories (MUST visible)】
- 珍珠耳钉
- 左腕：黑色方形智能手表（黑表带）
- 右腕：双层串珠手链（黑珠层 + 白/红珠层）
- 右手：银色细圈戒指
- 美甲：暗青色/暗青蓝色（dark teal / muted cyan）必须能看见
- 翡翠平安扣：细红绳系在立领内侧，平安扣微微露出一点（不要大坠子）
English: pearl drop earrings, black square smartwatch on LEFT wrist, double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand, dark teal / muted cyan nail polish clearly visible, small jade peace-buckle pendant on thin red string slightly peeking from inside mandarin collar.

【披肩（可选但希望有）/ Shawl (optional but preferred)】
- 白色透明纱质披肩：半透，雪纺/真丝纱，边缘银色细线暗纹
English: translucent sheer white silk gauze shawl over both shoulders, very thin chiffon with subtle silver embroidery along edges.

【四角道具小图标 / Corner props callouts (NOT held in hands)】
四角只放"物品小图标"，像说明书贴图，不让角色拿着。
(1) 左上：透明油纸伞（收拢）；半透米白伞面，淡青色竹枝，深色木柄
(2) 右上：水墨折扇（展开）；淡墨梅花/山水，黑檀木扇骨
(3) 左下：珍珠发簪（只要簪本体，不要流苏）；银质簪头，顶端单颗白珍珠
(4) 右下：织锦手包；深绿/酒红织锦缎，缠枝花纹，细银链
English:
upper left prop icon: closed translucent oil-paper umbrella, half-transparent ivory canopy, light cyan bamboo painting, dark wooden handle;
upper right prop icon: open black ink folding fan, faint plum blossom on rice paper, ebony ribs;
lower left prop icon: silver pearl hairpin, single white pearl on hairpin head, NO tassel, NO dangling pearl strands;
lower right prop icon: small dark green/burgundy brocade pouch, embroidered floral pattern, thin silver chain strap.

【背景 / Background】
- 湖心亭：中式凉亭 + 湖面涟漪 + 远处柳树 + 薄雾 + 阴天柔光
- 背景"弱化"，让主体最清晰
English: traditional chinese pavilion by misty lake, overcast soft grey sky, gentle water ripples, distant willow trees, light fog, peaceful contemplative atmosphere, soft natural overcast lighting, muted palette, background low contrast.

【画风 / Rendering style】
anime key visual style, clean line art, soft cel shading, highly detailed, official art style, 8k, ultra detailed, masterpiece, best quality.

【强约束 NEGATIVE】
multiple characters, character clones, twin views, split panels, multiple poses of same character, reference sheet grid with multiple body views,
hands holding any prop in the main pose, open umbrella above head in main pose,
tassels, dangling pearl strands, pearl necklaces-like decorations around hairpin,
busy background, strong contrast background, text overlay, watermark, signature, logo, distorted hands, extra fingers,
wrong buttons (traditional knotted frog buttons), missing smartwatch, missing bracelet, missing red bead buttons.
```

#### 9.3 使用建议

- 跑 4 张验证：模型能不能**一次性**守住 → 盘扣（圆珠）/ 手表（左腕）/ 手链（右腕）/ 坐姿 / 道具不被拿在手里
- 失败模式：模型把伞画到手里 → 把"四角道具"那段改为 `floating small product icons in corners, sticker-like callouts, not in hands`
- 优先级（保不住时按此顺序砍）：主体（旗袍盘扣+手表+坐姿）> 配饰 > 道具 > 背景

---

## 十一、姿态 × 场景组合 Prompt（坐姿 + 披肩 × 三场景）

> **用途**：测试模型在"同一姿态 + 同一服装/披肩 + 不同背景"下能否保持角色一致性。
> 三套 prompt 共享相同的"角色 / 服装 / 配饰 / 坐姿 / 披肩"描述块，**只换背景段**。
> 用于做：场景测试 / 多卡片立绘 / 季节切换 / 桌宠背景皮肤。

### 11.0 公共描述块（三个 prompt 都用）

```text
1girl, solo, full body, single character, character illustration,

young beautiful chinese woman around 20, refined intellectual vibe, calm and elegant neutral expression with subtle melancholy,
delicate east asian facial features, dark almond eyes, long lashes, defined eyebrows, red lips, fair skin,
medium-length warm chestnut brown hair, half-up hairstyle secured with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, mandarin collar, bodycon fit, matte fabric with subtle silk drape, side slit on skirt (the high side slit MUST remain clearly visible and unchanged in every scene, NOT covered by accessories, props, or the cat),
5 red round bead frog buttons on diagonal placket from collar to right hip (small red round beads, NOT traditional knotted frog buttons),
draped with translucent sheer white silk gauze shawl over both shoulders, very thin chiffon material, soft silver embroidery along the edges, shawl gently flowing past upper arms,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist, double-layer beaded bracelet (black beads + white/red beads) on RIGHT wrist, thin silver ring on right hand, dark teal nail polish clearly visible,

sitting elegantly on grey weathered stone bench, legs crossed slightly at ankles, body leaning back lightly, one hand resting on bench surface, other hand free for the cat (see per-scene cat block), pose and expression details (head direction / gaze) defined by each scene's cat interaction,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style, 8k, ultra detailed, masterpiece, best quality,

NEGATIVE: multiple characters, character clones, multiple poses of same character, hands holding any prop, opened umbrella, traditional knotted frog buttons, missing smartwatch, missing bracelet, tassels around hairpin, pearl strands hanging from hair, modern casual clothing, school uniform, watermark, text overlay, signature, logo, distorted hands, qipao side slit covered or hidden, qipao with no side slit, missing side slit.
```

### 11.1 场景 A · 湖心亭（阴天 · 静谧）

把以下背景段拼在公共块**之前**或**之后**均可：

```text
background scene: empty traditional chinese pavilion by misty lake, overcast soft grey sky, gentle water ripples on lake surface, distant willow trees with hanging branches, light fog drifting over water, modern minimalist architecture barely visible in haze far behind,
lighting: soft natural overcast lighting, no harsh shadows, evenly diffused light, cool muted color palette, peaceful contemplative atmosphere, low saturation pastel tones, ambient lake reflection light,
mood: 静谧、克制、清冷、有故事感, quiet and contemplative,

companion pet (sleeping on lap, default for this scene):
a small silver tabby american shorthair cat curled up sleeping on the character's lap, perfect loaf position with all paws tucked completely under body, eyes fully closed, peaceful relaxed sleepy expression, being perfectly still and quiet, body rounded into a soft compact shape,
silver gradient fur with extremely subtle almost invisible tabby stripes, subtle M-shaped marking on forehead, pinkish-brown nose, tail with dark ring markings curled around body, fluffy and soft,
cat rendered in the same anime key visual style as the character (clean line art, soft cel shading), matches her calm vibe and color palette,
IMPORTANT: the qipao side slit on the skirt MUST remain clearly visible and unchanged (cat rests only on the upper lap area, does NOT cover or hide the side slit),
```

### 11.2 场景 B · 江南雨巷（细雨 · 黄昏）

> **本场景特殊处理**：猫咪在旁边行走（不睡腿上），主角的眼神改为望向猫咪。
> 拼用时**覆盖**公共块里"head turned slightly to the side gazing softly into the distance"那一段，改成下面的「主角视线」描述。

```text
background scene: old narrow stone alley in southern china jiangnan style, wet bluestone ground reflecting warm lights, traditional white walls with black tiled roofs on both sides, slim bamboo plants along the wall, light drizzle / subtle falling rain captured as fine vertical strokes, distant red lanterns glowing warmly,
lighting: warm dim lantern light contrasting with cool wet stone, dusk golden-hour warmth, shallow depth of field, soft bokeh on distant lanterns, water reflections on the ground, gentle film grain,
mood: 诗意、怀旧、温柔忧郁, nostalgic poetic atmosphere,

character gaze override: head turned slightly downward to one side, eyes softly looking down at the cat walking beside her, gentle affectionate gaze, faint smile, attention on the cat (NOT gazing into distance),

companion pet (walking beside, NOT on lap): a small silver tabby american shorthair cat walking calmly on the wet stone ground beside the character (close to her feet on one side), all four paws on the ground, body in a relaxed walking posture with one front paw lifted mid-step, tail held softly upward with a gentle curve, ears perked up, bright curious eyes looking up toward the character (so the cat and character are looking at each other), slightly damp fluffy silver gradient fur with extremely subtle almost invisible tabby stripes, subtle M-shaped marking on forehead, pinkish-brown nose, tail with dark ring markings, well-behaved and quiet in the rain,
cat rendered in the same anime key visual style as the character (clean line art, soft cel shading, matching art style and color palette), the cat is on the ground beside her, NOT on her lap, NOT held in arms,

IMPORTANT - qipao side slit must remain unchanged: the side slit on the qipao skirt stays clearly visible and unchanged in this scene, the cat is on the ground beside the character (NOT touching the skirt, NOT covering the slit),
```

> 注：本场景下"坐姿+石凳"略反差（雨巷里通常没有石凳）。如果模型画不好，可把石凳换成
> `sitting on a small stone step at the side of the alley`（坐在巷口的小石阶上）。

### 11.3 场景 C · 雅室琴房（自然光 · 知性）

```text
background scene: minimalist chinese study room interior, solid dark wood guqin on long wooden table beside her, thin white gauze curtain by tall wooden window, soft natural daylight pouring in from window side, single potted orchid in plain ceramic pot, an ink wash scroll painting hanging on the rear wall, matte dark wood furniture, polished concrete or wooden floor,
lighting: warm soft natural daylight from window-side, gentle directional light with long soft shadows, warm and elegant atmosphere, low ambient saturation but slightly warmer than overcast scenes,
mood: 知性、安静、专注, intellectual and serene,
```

> 注：雅室琴房场景中可让她"坐在琴桌旁的木凳/蒲团上"，把公共块里的
> `grey weathered stone bench` 替换成 `low wooden bench beside guqin table` 更自然。

### 11.5 场景 D · 江南雨巷（持伞靠墙，站姿，镜头从右向前拍摄）

> **特殊说明**：本场景为站姿，与公共块的坐姿不同，需使用完整独立 prompt。

```text
background scene: old narrow stone alley in southern china jiangnan style, wet bluestone ground reflecting warm lantern lights, traditional white walls with black tiled roofs on both sides, slim bamboo plants along the wall, light drizzle / subtle falling rain captured as fine vertical strokes, distant red lanterns glowing warmly,
lighting: warm dim lantern light contrasting with cool wet stone, dusk golden-hour warmth, shallow depth of field, soft bokeh on distant lanterns, water reflections on the ground, gentle film grain,
mood: 诗意、怀旧、温柔忧郁, nostalgic poetic atmosphere,

1girl standing in the alley leaning against the white wall,
wearing dark navy blue sleeveless modified qipao dress, mandarin collar, bodycon fit, matte fabric with subtle silk drape, side slit on skirt (the side slit MUST remain clearly visible and unchanged),
5 red round bead frog buttons on diagonal placket from collar to right hip (small red round beads, NOT traditional knotted frog buttons),
right hand holding an open translucent oil-paper umbrella with light cyan bamboo pattern, umbrella handle resting against her right shoulder,
left hand gently placed on the white wall beside her,
body angled slightly, facing towards the camera with a calm and gentle expression,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist, double-layer beaded bracelet (black beads + white/red beads) on RIGHT wrist, thin silver ring on right hand, dark teal nail polish clearly visible,
medium-length warm chestnut brown hair, half-up hairstyle secured with pearl hairpins, loose face-framing strands,

camera angle: shot from the right side moving forward, capturing the character in a three-quarter view,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style, 8k, ultra detailed, masterpiece, best quality,

NEGATIVE: multiple characters, character clones, hands holding other props, traditional knotted frog buttons, missing smartwatch, missing bracelet, tassels around hairpin, modern casual clothing, watermark, text overlay, signature, logo, distorted hands, qipao side slit covered or hidden.
```

### 11.6 场景 E · 江南雨巷（背向行走，人与猫视线交会）

> **特殊说明**：本场景为行走姿态，与公共块的坐姿不同，需使用完整独立 prompt。

```text
background scene: old narrow stone alley in southern china jiangnan style, wet bluestone ground reflecting warm lantern lights, traditional white walls with black tiled roofs on both sides, slim bamboo plants along the wall, light drizzle / subtle falling rain captured as fine vertical strokes, distant red lanterns glowing warmly,
lighting: warm dim lantern light contrasting with cool wet stone, dusk golden-hour warmth, shallow depth of field, soft bokeh on distant lanterns, water reflections on the ground, gentle film grain,
mood: 诗意、怀旧、温柔忧郁, nostalgic poetic atmosphere,

1girl walking forward in the alley with her back to the camera, positioned on the right side of the alley,
wearing dark navy blue sleeveless modified qipao dress, mandarin collar, bodycon fit, matte fabric with subtle silk drape, side slit visible from behind (the side slit MUST remain clearly visible and unchanged),
5 red round bead frog buttons on diagonal placket from collar to right hip (small red round beads, NOT traditional knotted frog buttons),
right hand holding an open translucent oil-paper umbrella with light cyan bamboo pattern, umbrella handle resting against her right shoulder,
flowing dark hair with pearl hairpins visible from behind,

small silver tabby american shorthair cat walking forward on the left side of the alley,
cat with silver gradient fur, extremely subtle almost invisible tabby stripes, tail held gently upward,
cat's head turned back towards the character, looking over its shoulder,

character's head slightly turned to her left, looking back at the cat over her shoulder,
eyes meeting the cat's gaze in a gentle connection,

both character and cat rendered in anime key visual style, clean line art, soft cel shading, highly detailed, official art style, 8k, ultra detailed, masterpiece, best quality,

NEGATIVE: multiple characters, character clones, hands holding other props, traditional knotted frog buttons, missing smartwatch, missing bracelet, tassels around hairpin, modern casual clothing, watermark, text overlay, signature, logo, distorted hands, qipao side slit covered or hidden.
```

### 11.4 使用提示

- **三套是兄弟版**：仅替换 §11.1/11.2/11.3 的 `background scene + lighting + mood` 三段，其他完全一致。便于做"同角色不同场景"的横向对比。
- **桌宠用法**：把这三张图作为桌宠"背景皮肤"，根据时间/天气切换 —— 如下雨切 11.2、专注工作切 11.3、待机切 11.1。
- **若想要"披肩飘起来"**：把公共块里 `shawl gently flowing past upper arms` 改成 `shawl gently fluttering in the breeze, edges lifting softly` —— 11.2 雨巷场景配合微风效果最佳。
- **披肩透明感失败**：模型常把"半透雪纺"画成"白色厚布"。可加 `shawl: see-through, you can faintly see the qipao shoulder through the fabric`。

---

## 十二、桌宠子形象 Prompt（第一批）

> **用途**：为桌宠运行时的不同语义状态生成同一角色的子形象素材。所有子形象必须保持原角色服装、发型、配饰与整体气质不变，只通过姿态、表情、手部动作、工作道具和临时状态道具表达差异。
>
> **统一硬约束**：必须保留 `medium-length brown hair, half-up hairstyle with pearl hairpins`、`dark navy blue sleeveless modified qipao dress`、`5 red round bead buttons on diagonal placket`、`black square smartwatch on LEFT wrist`、`double-layer beaded bracelet on RIGHT wrist`、`pearl drop earrings`、`thin silver ring`、`dark teal / muted cyan nail polish`。
>
> **版本规则**：每个子形象提供「正式版」与「Q 版」两份。正式版用于主素材 / Live2D 精细绑定；Q 版用于小尺寸桌宠、状态图标和轻量动画。Q 版也必须保留同一服装、发型和关键配饰。
>
> **工作姿态硬约束**：除 `programmer.agent_leader` 外，所有子形象都必须是 `sitting at a desk / seated at a work desk` 的桌前工作姿态，允许后躺、前倾、挥毫、打字等坐姿变化，但禁止站姿、行走姿态和无桌面悬浮姿态。
>
> **agent_leader 特例**：`programmer.agent_leader` 必须是全身站姿，使用皇帝登基式姿态；该子形象不适用桌前坐姿约束，也不使用统一 Negative Prompt 中的 `standing pose` 禁止项。

### 12.1 original.video

语义：原设计唯一子形象；默认、常驻、等待都使用该视频。

#### 12.1.1 正式版

```text
1girl, solo, live2d desktop pet idle video, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, calm and elegant neutral expression,
delicate east asian facial features, dark almond eyes, red lips, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

calm idle seated pose at a clean work desk, body facing forward, head upright,
hands relaxed and naturally resting on the desktop near lower frame,
quiet breathing motion, occasional soft blinking, subtle hair sway,
cool and elegant new chinese style aesthetic, quiet companion feeling while seated at desk,

solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D desktop pet idle state,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, casual clothes, school uniform, standing pose, walking pose, no desk, missing work desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, tassels, pearl strands hanging from hair, exaggerated gesture, holding tools, busy background, text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.1.2 Q 版

```text
chibi 1girl, solo, live2d chibi desktop pet idle sprite, upper body or full body,

same young chinese woman as chibi, large head small body, cute round dark eyes, small red lips,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

calm cute idle seated pose at a tiny clean work desk, tiny relaxed hands resting on the desktop,
soft blinking, gentle breathing, quiet companion feeling while seated at desk,
chibi anime style, clean line art, soft cel shading, cute but elegant new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, tassels, messy background, text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.2 document_secretary.thinking

语义：文档秘书思考 / 摸鱼 / 常驻；传统笔墨纸砚，道具为毛笔。摸鱼状态：嘟嘴，毛笔横放在嘴上。

#### 12.2.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, playful thoughtful expression,
delicate east asian facial features, dark almond eyes looking slightly upward, red lips in a small pout, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary idle thinking state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
she is pouting slightly, holding a calligraphy brush horizontally near or gently against her lips like a playful moustache,
one elbow resting lightly on the desk near the paper, relaxed daydreaming seated posture, elegant but slacking off,
traditional brush and ink atmosphere, quiet humorous contrast, no modern office tools,

new chinese style aesthetic, cool elegant temperament with subtle playful charm,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, messy papers, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.2.2 正式版 · 双手撑脸

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, cute playful thoughtful expression,
delicate east asian facial features, dark almond eyes looking directly at the camera, soft sparkling eyes, gentle red lips, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary cute thinking state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
both hands supporting her cheeks, elbows resting on the desktop,
looking directly at the camera with adorable attention-seeking expression, cute but elegant, gentle moe feeling,
a calligraphy brush placed on a brush rest on the desk, NOT held in the mouth, NOT held in hands,
traditional brush and ink atmosphere, quiet humorous contrast, no modern office tools,

new chinese style aesthetic, cool elegant temperament with subtle playful charm,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, messy papers, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.2.3 Q 版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, cute dark eyes, playful thoughtful expression,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary cute thinking pose,
seated at a tiny traditional writing desk,
both tiny hands supporting her cheeks, elbows resting on the desktop,
looking directly at the camera with cute sparkling eyes, playful moe expression, adorable attention-seeking mood,
a small calligraphy brush placed on a brush rest on the desk, NOT held in the mouth, NOT held in hands,
small inkstone, ink stick, rice paper and brush rest on the desk, cute traditional stationery props,
funny but elegant, idle thinking mood,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.3 document_secretary.writing

语义：文档秘书写作 / 整理文档 / 生成文案；写狂草的姿态，自信、潇洒。

#### 12.3.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body or three-quarter body,

young beautiful chinese woman around 20, refined intellectual vibe, confident free-spirited smile, relaxed and dashing expression,
delicate east asian facial features, dark almond eyes with a confident smiling gaze, red lips curved into a clear gentle smile, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary writing state with traditional brush calligraphy,
seated at a traditional writing desk, writing wild cursive calligraphy on long rice paper with a calligraphy brush,
right hand sweeping the brush in a bold flowing arc, left hand pressing the rice paper steady on the desk,
confident and unrestrained seated posture, graceful shoulder line, dynamic sleeve-free arm movement,
inkstone, ink stick, brush rest and paperweight arranged on the desk,
visible black ink strokes as abstract flowing calligraphy shapes, no readable text,
elegant, self-assured, carefree and dashing energy, smiling confidently while writing, NOT serious, NOT stern,

new chinese style aesthetic, cool elegant temperament, refined intellectual vibe,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, ballpoint pen, modern notebook, laptop, readable text, messy ink splashes covering outfit, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.3.2 Q 版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, bright confident eyes, smug cute smile,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary writing wild cursive calligraphy,
seated at a tiny traditional writing desk,
holding an oversized calligraphy brush, sweeping it dramatically across long rice paper on the desk,
small inkstone, ink stick and paperweight beside her on the desktop,
confident dashing seated pose, tiny body leaning into the brush stroke, cute ink swoosh effects,
abstract black ink strokes only, no readable text,

chibi anime style, clean line art, soft cel shading, cute energetic new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, modern pen, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.4 programmer.thinking

语义：程序员思考 / 摸鱼 / 常驻；显示器、键盘、鼠标、人体工学椅。摸鱼状态：人体工学椅后躺，双手反扣在脑后，舒展。

#### 12.4.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body or three-quarter body,

young beautiful chinese woman around 20, refined intellectual vibe, relaxed satisfied expression,
delicate east asian facial features, dark almond eyes half closed, faint lazy smile, red lips, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer idle thinking / slacking off state,
sitting in a modern ergonomic chair, leaning far back comfortably,
both hands clasped behind the head, elbows spread outward, stretching and relaxing,
left wrist with black square smartwatch visible near hair, right wrist with double-layer beaded bracelet visible near hair,
computer monitor, compact keyboard and mouse on desk in front of her, monitor shows abstract blurred code blocks with no readable text,
comfortable developer workstation, calm relaxed break after thinking,

new chinese style aesthetic fused with minimal modern programmer workspace,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, hoodie, cyberpunk outfit, school uniform, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, bed, sofa, messy cables, readable code text, brand logos, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.4.2 Q 版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, relaxed sleepy eyes, cute smug smile,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi programmer slacking off in an ergonomic chair,
chair tilted backward, tiny body reclining with lazy half-lidded eyes,
only two arms and two hands visible: LEFT hand placed behind the head, RIGHT hand resting naturally on the lap or chair armrest,
one elbow raised behind the head, the other arm relaxed downward, asymmetrical relaxed pose,
small monitor, keyboard and mouse on desk nearby, abstract code blocks on screen, no readable text,
comfy stretching pose, cute lazy idle mood, same pose quality as the reference image but anatomically correct,

chibi anime style, clean line art, soft cel shading, cute modern developer workspace,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, sofa, bed, readable text, watermark, logo, distorted hands, extra fingers, extra arms, third hand, three hands, duplicated hand, duplicated arm, hand growing from shoulder, hand growing from hair, hand behind both sides of head, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.5 programmer.agent_leader

语义：控制 Claude Code / Codex / subagent，等待或汇总子 agent 输出。特殊状态道具：头顶参考图同款金色扁平冕冠发饰，戴着部分反光墨镜；皇帝登基姿态，双手平举。

#### 12.5.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body or full body,

young beautiful chinese woman around 20, refined intellectual vibe, confident commanding expression,
delicate east asian facial features, dark almond eyes partly hidden behind semi-reflective sunglasses, red lips, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands still visible,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

temporary comedic agent leader props layered over the original design:
a miniature golden traditional chinese emperor mian crown hair ornament placed on top of the head without changing the hairstyle,
the crown is a flat horizontal rectangular golden top plaque with ornate gold rim, short front bead curtain, multiple dangling strings of tiny golden beads, each string ending with a small white pearl drop, like the reference image,
semi-reflective sunglasses worn low on the face, eyes partly visible through the lenses,
programmer agent leader ascension pose, full body standing pose,
standing upright like an emperor taking the throne, majestic ceremonial posture, both arms raised and held horizontally forward and outward in a commanding gesture,
left wrist smartwatch and right wrist bracelet clearly visible on the raised arms,
small floating terminal panels, task cards and abstract AI agent nodes arranged like ministers around her,
panels are symbolic with no readable text and no brand logos,
confident, majestic, slightly humorous, commanding multiple subagents,

new chinese style aesthetic with subtle modern AI command interface,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy dress with red bead accents,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, sitting pose, cropped body, half body, cyber armor, dragon robe replacing qipao, missing qipao, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, fully opaque sunglasses hiding all face, changing hairstyle, readable terminal text, Claude logo, Codex logo, company logos, busy UI, watermark, signature, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.5.2 Q 版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, smug commanding smile,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing a tiny golden traditional chinese emperor mian crown hair ornament on top of the head without changing hairstyle,
the crown is a flat horizontal rectangular golden top plaque with short dangling golden bead strings and tiny white pearl drops along the front edge, like the reference image,
wearing small semi-reflective sunglasses low on the face, eyes partly visible,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi programmer agent leader ascension pose, full body standing pose,
both tiny arms raised and held horizontally outward like an emperor giving commands,
small floating terminal panels, task cards and cute abstract AI agent nodes around her like little ministers,
no readable text, no brand logos,
majestic but funny, confident subagent commander mood,

chibi anime style, clean line art, soft cel shading, cute new chinese imperial comedy style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, sitting pose, cropped body, half body, dragon robe replacing qipao, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, fully opaque sunglasses, changing hairstyle, readable text, logos, watermark, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.6 programmer.coding

语义：写代码 / 生成补丁 / 处理 coding 输出；显示器、键盘、鼠标、人体工学椅。Coding 状态：双手打键盘，身体前倾靠近显示器。

#### 12.6.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body or three-quarter body,

young beautiful chinese woman around 20, refined intellectual vibe, intensely focused coding expression,
delicate east asian facial features, dark almond eyes looking toward monitor, red lips, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer coding state at a modern workstation,
sitting in an ergonomic chair, body leaning forward close to the monitor,
both hands typing rapidly on a compact keyboard, mouse beside the keyboard,
left wrist smartwatch clearly visible while typing, right wrist double-layer beaded bracelet clearly visible while typing,
large monitor in front of her showing abstract code blocks and patch diff shapes, no readable text,
focused and efficient posture, generating patches, writing clean code, processing coding output,
subtle motion feeling in fingers, but hands remain elegant and well-formed,

new chinese style aesthetic with minimal modern developer tools,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, hoodie, office suit, casual clothing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop covering qipao, hands hidden, unreadable messy fingers, readable code text, brand logos, watermark, signature, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.6.2 Q 版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, serious focused eyes, tiny determined mouth,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi programmer coding at workstation,
sitting in a small ergonomic chair, leaning forward very close to a big monitor,
both tiny hands rapidly typing on a compact keyboard, mouse beside keyboard,
abstract code blocks and patch diff shapes on monitor, no readable text,
focused coding energy, cute intense concentration,

chibi anime style, clean line art, soft cel shading, cute modern developer workspace,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop covering body, readable text, logos, watermark, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.7 programmer.error

语义：程序出错/工具调用失败，双手托下巴盯着显示器，抿嘴生气，头顶冒火苗。

#### 12.7.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, angry pouting expression, silently furious,
delicate east asian facial features, dark almond eyes glaring at the monitor, eyebrows furrowed angrily, red lips pursed tightly together in a pout, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer error / tool failure state at a modern workstation,
sitting in an ergonomic chair leaning forward toward the desk,
both hands supporting her chin, elbows resting on the desk, palms cupping the face on both sides,
staring at the computer monitor with pursed lips and a visibly annoyed frown,
computer monitor, keyboard and mouse on desk in front, monitor shows abstract error-like visual patterns, no readable text,
anime-style stylized flame marks floating above her head, small cartoon-like fire shapes symbolizing simmering anger,
frustrated and silently furious, holding in anger, not crying, not sighing,

new chinese style aesthetic with minimal modern developer tools,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, hoodie, crying, shouting, open mouth yelling, large tears, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, hands covering face, readable code text, brand logos, watermark, signature, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.7.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, angry pouting expression, cute furious glare,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi programmer error state at workstation,
sitting in a small ergonomic chair leaning forward,
both tiny hands supporting the chin, elbows on the desk, palms cupping both cheeks,
staring at the small monitor with an angry pout, cute furious expression,
small monitor and keyboard on desk, abstract error patterns on screen, no readable text,
chibi-style flame marks floating above her head, tiny cute fire symbols,
annoyed but adorable, silently angry, not crying,

chibi anime style, clean line art, soft cel shading, cute modern developer workspace,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, crying, tears, open mouth laughing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, hands covering face, readable text, logos, watermark, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.8 programmer.done

语义：任务完成 / 代码通过，右手食指竖在唇前，俯瞰镜头，自信从容。

#### 12.8.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, confident smug expression, calm and composed,
delicate east asian facial features, dark almond eyes looking down at the camera from slightly above, half-lidded confident gaze, red lips curved into a slight gentle smile, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer task complete / code passed state, showing off with quiet confidence,
sitting in an ergonomic chair leaning back slightly, relaxed and in control,
right hand raised, index finger placed vertically at the center of her lips, fingertip touching the middle point between upper and lower lip, remaining fingers curled inward,
left hand resting naturally on the desk or armrest,
looking down at the viewer with a half-lidded confident gaze, slight gentle smile, as if saying "too easy",
computer monitor, keyboard and mouse on desk in front, monitor shows abstract green check or pass visual, no readable text,
confident and composed, gentle victorious smile, cool and collected,

new chinese style aesthetic with minimal modern developer tools,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, hoodie, smiling happily, big smile, wide open eyes, shouting, cheering, raised fist, both hands on head, hands behind head, two hands raised, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable code text, brand logos, watermark, signature, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.8.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, confident slight smile, half-lidded eyes looking down at viewer,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi programmer task complete state,
sitting in a tiny ergonomic chair,
right tiny hand raised with index finger pressed vertically at the center of the lips, fingertip touching the middle point between upper and lower lip,
left tiny hand resting on the desk or lap,
looking down at the viewer with a confident cute slight smile,
small monitor on desk showing abstract checkmark pattern, no readable text,
confident cute energy with a gentle smile, job well done showing off,

chibi anime style, clean line art, soft cel shading, cute modern developer workspace,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing workstation desk, smiling happily, big smile, both hands raised, hands behind head, shouting, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable text, logos, watermark, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.8.3 半身像 · 无道具（bust portrait, no props）

> 用途：头像 / 状态图标 / 小尺寸展示。仅保留角色 + 配饰 + "嘘"手势，删去一切桌椅、显示器等道具。

```text
1girl, solo, live2d desktop pet sprite, bust portrait from chest up, close-up,

young beautiful chinese woman around 20, refined intellectual vibe, proud and arrogant expression, chin slightly raised, looking straight at the viewer at eye level,
delicate east asian facial features, dark almond eyes looking straight at the camera at eye level, half-lidded haughty gaze, red lips curved into a smug confident smirk with lips closed no teeth showing, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

the character's own right hand raised near face, index finger placed vertically at the center of her lips, fingertip touching the middle point between upper and lower lip, remaining fingers curled inward,
left hand hanging naturally at her side, relaxed downward,
looking straight at the viewer at eye level with a half-lidded haughty gaze, smug triumphant smirk with lips closed no teeth showing, as if the viewer is beneath her notice,
no desk, no chair, no monitor, no keyboard, no mouse, no workspace props, character only,

new chinese style aesthetic, cool elegant temperament,
solid pure white background, no shadows, no gradients, isolated character, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, desk, chair, monitor, keyboard, mouse, computer, workstation, any props, tools, hoodie, smiling happily, big smile, open mouth, teeth, showing teeth, grinning, wide open eyes, shouting, cheering, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable text, brand logos, watermark, signature, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow, looking down, looking up, bird eye view, worm eye view, low angle, high angle
```

```text
chibi 1girl, solo, live2d chibi desktop pet sprite, bust portrait from chest up, close-up,

same young chinese woman as chibi, large head small body, proud arrogant expression, chin raised, smug haughty eyes looking straight at viewer at eye level,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

the character's own right tiny hand raised with index finger pressed vertically at the center of the lips, fingertip touching the middle point between upper and lower lip,
left tiny hand hanging naturally at her side, relaxed downward,
looking straight at the viewer at eye level with a smug triumphant cute smirk with lips closed no teeth showing, chin raised proudly,
no desk, no chair, no monitor, no props, character only,
proud arrogant cute energy, as if the viewer is beneath her notice,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, desk, chair, monitor, keyboard, mouse, any props, smiling happily, big smile, open mouth, teeth, showing teeth, grinning, both hands raised, hands behind head, shouting, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable text, logos, watermark, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow, looking down, looking up, bird eye view, worm eye view, low angle, high angle
```

### 12.9 document_secretary.idle

语义：桌前静坐，平和表情，等待任务；文房场景，平静等待。

#### 12.9.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, calm and peaceful neutral expression,
delicate east asian facial features, dark almond eyes softly open, gentle red lips, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary idle / waiting state with traditional chinese stationery,
seated at a traditional writing desk, ink brush placed neatly on brush rest,
inkstone, ink stick, rice paper, paperweight arranged neatly on desk,
both hands resting gently on the desktop near the lower frame,
body facing forward, head upright, calm relaxed seated posture,
breathing softly, waiting quietly, peaceful and elegant,
no active writing, no tools in hands, just sitting quietly ready,

new chinese style aesthetic, cool elegant temperament, quiet waiting mood,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, holding tools, active writing pose, laptop, keyboard, modern pen, messy papers, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.9.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, calm soft eyes, gentle tiny smile,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary idle state,
seated at a tiny traditional writing desk,
both tiny hands resting on the desktop,
ink brush placed neatly on brush rest, inkstone, rice paper arranged,
calm relaxed waiting pose, peaceful and elegant,
cute quiet companion feeling,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, holding tools, active writing, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.10 document_secretary.error

语义：文档处理卡住想不通，笔杆塞嘴里思考，头顶冒问号。

#### 12.10.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, thoughtful confused expression, deep in thought,
delicate east asian facial features, dark almond eyes looking upward in contemplation, eyebrows slightly raised, red lips gently holding a brush handle, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary stuck thinking state with traditional chinese stationery,
seated at a traditional writing desk,
right hand holding a calligraphy brush, the brush handle gently held between her lips like a thinking gesture,
left hand resting on the desk, fingers lightly tapping or touching the rice paper,
inkstone, ink stick and partially written rice paper on the desk,
eyes looking upward toward the ceiling or into space, pondering deeply,
question mark symbols floating above her head, one large and a few small,
confused but calm, not frustrated, just genuinely cannot figure it out,
peaceful puzzled expression, elegant even when confused,

new chinese style aesthetic, cool elegant temperament with gentle confusion,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, angry expression, frustrated expression, crying, shouting, open mouth wide, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, both hands on head, brush in mouth like a cigar, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.10.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, thoughtful confused eyes looking upward, tiny brush handle between lips,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary stuck thinking state,
seated at a tiny traditional writing desk,
tiny hand holding a calligraphy brush, the handle gently held between her lips,
thinking pose with eyes looking upward, confused pondering expression,
inkstone and rice paper on desk,
question mark symbols floating above her head, cute and funny,
confused but adorable, not frustrated, just thinking hard,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, angry expression, frustrated expression, crying, shouting, brush in mouth like cigar, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.11 document_secretary.done

语义：任务完成，满意微笑，放下笔，放松满足。

#### 12.11.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, gentle satisfied smile,
delicate east asian facial features, dark almond eyes curved with warm soft happy expression, red lips curved into a clear gentle contented smile, fair skin,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary task complete state with traditional chinese stationery,
seated at a traditional writing desk, task just finished,
calligraphy brush placed down on brush rest, both hands now relaxed and withdrawn from writing,
one hand resting on the desk, the other hand lightly on her lap or desk edge,
body relaxing back slightly after completing the work,
inkstone, ink stick, and finished rice paper with abstract flowing ink strokes on the desk,
warm satisfied smile, job well done, peaceful and content,
no readable text on the paper, abstract ink art only,

new chinese style aesthetic, cool elegant temperament with gentle warmth,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, intense expression, wide open eyes, shouting, fist pump celebration, still holding brush, active writing pose, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.11.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, happy satisfied squinting eyes, cute warm smile,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary task complete state,
seated at a tiny traditional writing desk,
brush placed down on brush rest, both tiny hands released from writing,
relaxed cute satisfied pose, warm contented smile,
inkstone and finished rice paper with abstract ink strokes on desk,
job well done cute energy, peaceful satisfaction,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, still holding brush, intense expression, shouting, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.12 document_secretary.shy (脸红害羞)

语义：工具失败时的害羞表情；双手不持笔，桌上文房四宝整齐摆放，脸红害羞、眼神回避。

#### 12.12.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, shy embarrassed expression, cheeks deeply flushed with pink blush,
delicate east asian facial features, dark almond eyes looking away shyly or down to the side, long eyelashes, red lips in a small shy pout, fair skin with visible pink blush across the nose and cheeks,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary shy / embarrassed state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
calligraphy brush placed on brush rest on the desk, NOT held in hands,
both hands resting gently on the desk or one hand lightly touching her cheek,
eyes looking away or downward, avoiding eye contact, shy embarrassed demeanor,
cheeks and nose visibly flushed with pink warm blush, slightly tilted head,
caught off guard or embarrassed by a mistake, adorable shy reaction, elegant even when flustered,

new chinese style aesthetic, cool elegant temperament with shy charm,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, angry expression, crying, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, messy papers, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.12.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, shy embarrassed expression, bright pink blush on cheeks,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary shy state,
seated at a tiny traditional writing desk,
small calligraphy brush placed on brush rest on the desk, NOT held in hands,
both tiny hands resting on the desk or one hand lightly touching her cheek,
cheeks flushed bright pink, eyes looking away shyly or down, avoiding the camera,
cute embarrassed expression, caught off guard, adorable flustered mood,
small inkstone and rice paper on desk,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.13 document_secretary.shy_peek (捂脸偷看 · 脸红害羞)

语义：工具失败时的害羞表情；双手捂脸，但眼睛从手指缝偷看，脸红。

#### 12.13.1 正式版

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,

young beautiful chinese woman around 20, refined intellectual vibe, extremely shy and flustered expression, cheeks deeply flushed with warm pink blush spreading across the face,
delicate east asian facial features, dark almond eyes peeking out through slightly parted fingers, visible shy curious gaze, red lips hidden behind hands, fair skin with prominent pink blush visible on exposed cheek areas,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,

pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary shy / hiding face state with traditional chinese stationery,
seated at a traditional writing desk,
both hands raised to cover the lower half of the face in a classic anime shy gesture,
fingers slightly apart, eyes clearly visible peeking through the finger gaps, looking at the camera or person,
cheeks and exposed skin visibly flushed with pink blush, embarrassed but curious,
ink brush, inkstone, rice paper and paperweight on the desk behind her hands,
caught off guard, flustered but secretly wanting to see the reaction,
elegant even when hiding, adorable peek-a-boo shy expression,

new chinese style aesthetic, cool elegant temperament with adorable shyness,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette, suitable for Live2D rigging,
anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, office suit, casual clothing, hands covering eyes completely, hands covering entire face with no visible eyes, angry expression, crying, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

#### 12.13.2 Q版

```text
chibi 1girl, solo, live2d chibi desktop pet sprite,

same young chinese woman as chibi, large head small body, shy peeking expression, bright pink blush on cheeks,
medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, tiny black square smartwatch on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring, dark teal nail polish,

chibi document secretary shy hiding state,
seated at a tiny traditional writing desk,
both tiny hands raised to cover the lower face, no pen or brush in hands,
fingers slightly parted, cute big eyes peeking through the gaps,
cheeks bright pink with blush, embarrassed but curious peek-a-boo expression,
small inkstone and rice paper on the desk,
adorably flustered, elegant even when hiding,

chibi anime style, clean line art, soft cel shading, cute new chinese style,
solid pure white background, no shadows, no gradients, isolated character and props, clean cutout ready,
masterpiece, best quality, highly detailed,

NEGATIVE: different outfit, standing pose, walking pose, no desk, hands covering eyes completely, no visible eyes, angry expression, crying, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow
```

### 12.14 「Q版脸 + 正式版身材」变体 Prompt（全子形象）

> **用途**：介于 Q 版和正式版之间的「2.5 头身」混合风格 —— 头部保留 Q 版的大圆眼、圆脸、可爱表情，
> 身体用正式版的写实比例、姿态细节和服装配饰精度。适合中等尺寸桌宠、社交媒体头像、表情包素材。
>
> **统一体型描述**（所有变体共享）：
> `semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 to 1/4 of total body height), round cute face with large expressive eyes, but body retains realistic adult female proportions, slender arms, detailed hands and fingers, elegant posture — a "grown-up body with a cute face" hybrid style`
>
> **统一 Negative 追加**：在各子形象原有 Negative 基础上追加：
> `full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, sharp angular jawline`

#### 12.14.1 original.video

```text
1girl, solo, live2d desktop pet idle sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large dark expressive eyes, small nose, tiny red lips, gentle soft expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

calm idle seated pose at a clean work desk, body facing forward, head upright,
hands relaxed and naturally resting on the desktop near lower frame,
quiet breathing motion, occasional soft blinking,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, sharp angular jawline, different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, tassels, messy background, text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.2 document_secretary.thinking（毛笔横放嘴上 · 摸鱼）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large dark expressive eyes looking slightly upward, small pouty red lips, playful thoughtful expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary idle thinking state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
she is pouting slightly, holding a calligraphy brush horizontally near or gently against her lips like a playful moustache,
one elbow resting lightly on the desk near the paper, relaxed daydreaming seated posture, elegant but slacking off,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.3 document_secretary.thinking（双手撑脸 · 卖萌）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large sparkling dark eyes looking directly at camera, small red lips, adorable attention-seeking expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary cute thinking state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
both hands supporting her cheeks, elbows resting on the desktop,
looking directly at the camera with adorable attention-seeking expression, cute but elegant, gentle moe feeling,
a calligraphy brush placed on a brush rest on the desk, NOT held in the mouth, NOT held in hands,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.4 document_secretary.writing（写狂草 · 潇洒）

```text
1girl, solo, live2d desktop pet sprite, upper body or three-quarter body,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large bright confident eyes, smug cute smile, small red lips curved into a grin,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary writing state with traditional brush calligraphy,
seated at a traditional writing desk, writing wild cursive calligraphy on long rice paper with a calligraphy brush,
right hand sweeping the brush in a bold flowing arc, left hand pressing the rice paper steady on the desk,
confident and unrestrained seated posture, graceful shoulder line, dynamic sleeve-free arm movement,
inkstone, ink stick, brush rest and paperweight arranged on the desk,
visible black ink strokes as abstract flowing calligraphy shapes, no readable text,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, ballpoint pen, modern notebook, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.5 programmer.thinking（后躺摸鱼 · 双手反扣脑后）

```text
1girl, solo, live2d desktop pet sprite, upper body or three-quarter body,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large relaxed sleepy eyes, faint lazy smug smile, small red lips,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer idle thinking / slacking off state,
sitting in a modern ergonomic chair, leaning far back comfortably,
both hands clasped behind the head, elbows spread outward, stretching and relaxing,
left wrist with black square smartwatch visible near hair, right wrist with double-layer beaded bracelet visible near hair,
computer monitor, compact keyboard and mouse on desk in front of her, monitor shows abstract blurred code blocks with no readable text,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing workstation desk, hoodie, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, sofa, bed, readable text, watermark, logo, distorted hands, extra fingers, extra arms, third hand, colored background, shadow on background, floor shadow
```

#### 12.14.6 programmer.agent_leader（皇帝登基 · 站姿特例）

```text
1girl, solo, live2d desktop pet sprite, upper body or full body,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large dark eyes partly hidden behind semi-reflective sunglasses, smug commanding cute smile, small red lips,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands still visible,
wearing a miniature golden traditional chinese emperor mian crown hair ornament on top of the head without changing hairstyle,
the crown is a flat horizontal rectangular golden top plaque with short dangling golden bead strings and tiny white pearl drops along the front edge,
semi-reflective sunglasses worn low on the face, eyes partly visible through the lenses,

wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer agent leader ascension pose, full body standing pose,
standing upright like an emperor taking the throne, majestic ceremonial posture, both arms raised and held horizontally forward and outward in a commanding gesture,
left wrist smartwatch and right wrist bracelet clearly visible on the raised arms,
small floating terminal panels, task cards and abstract AI agent nodes arranged like ministers around her,
panels are symbolic with no readable text and no brand logos,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy dress with red bead accents,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, sitting pose, cropped body, half body, dragon robe replacing qipao, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, fully opaque sunglasses, changing hairstyle, readable text, logos, watermark, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.7 programmer.coding（打键盘 · 专注）

```text
1girl, solo, live2d desktop pet sprite, upper body or three-quarter body,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large serious focused dark eyes, tiny determined mouth, small red lips pressed together,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer coding state at a modern workstation,
sitting in an ergonomic chair, body leaning forward close to the monitor,
both hands typing rapidly on a compact keyboard, mouse beside the keyboard,
left wrist smartwatch clearly visible while typing, right wrist double-layer beaded bracelet clearly visible while typing,
large monitor in front of her showing abstract code blocks and patch diff shapes, no readable text,
focused and efficient posture, generating patches, writing clean code,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing workstation desk, hoodie, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop covering qipao, readable code text, brand logos, watermark, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.8 programmer.error（生气冒火 · 双手托下巴）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large angry glaring dark eyes, eyebrows furrowed, small red lips pursed tightly in a pout, cute furious expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer error / tool failure state at a modern workstation,
sitting in an ergonomic chair leaning forward toward the desk,
both hands supporting her chin, elbows resting on the desk, palms cupping the face on both sides,
staring at the computer monitor with pursed lips and a visibly annoyed frown,
computer monitor, keyboard and mouse on desk in front, monitor shows abstract error-like visual patterns, no readable text,
anime-style stylized flame marks floating above her head, small cartoon-like fire shapes symbolizing simmering anger,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing workstation desk, crying, shouting, open mouth yelling, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, hands covering face, readable code text, brand logos, watermark, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.9 programmer.done（嘘手势 · 俯瞰自信）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large half-lidded confident dark eyes looking down at viewer, smug triumphant cute smirk with lips closed, small red lips,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer task complete / code passed state, showing off with quiet confidence,
sitting in an ergonomic chair leaning back slightly, relaxed and in control,
right hand raised, index finger placed vertically at the center of her lips, fingertip touching the middle point between upper and lower lip, remaining fingers curled inward,
left hand resting naturally on the desk or armrest,
looking down at the viewer with a half-lidded confident gaze, slight gentle smile,
computer monitor, keyboard and mouse on desk in front, monitor shows abstract green check or pass visual, no readable text,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing workstation desk, smiling happily, big smile, wide open eyes, both hands raised, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable code text, brand logos, watermark, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.10 programmer.done（半身像 · 无道具）

```text
1girl, solo, live2d desktop pet sprite, bust portrait from chest up, close-up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large half-lidded haughty dark eyes looking straight at viewer at eye level, smug triumphant cute smirk with lips closed no teeth showing, small red lips, chin raised proudly,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

the character's own right hand raised near face, index finger placed vertically at the center of her lips, fingertip touching the middle point between upper and lower lip, remaining fingers curled inward,
left hand hanging naturally at her side, relaxed downward,
looking straight at the viewer at eye level with a half-lidded haughty gaze, smug triumphant smirk,
no desk, no chair, no monitor, no keyboard, no mouse, no workspace props, character only,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, desk, chair, monitor, keyboard, mouse, any props, tools, smiling happily, big smile, open mouth, teeth, showing teeth, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable text, brand logos, watermark, distorted hands, extra fingers, colored background, shadow on background, floor shadow, looking down, looking up
```

#### 12.14.11 document_secretary.idle（静坐等待）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large calm soft dark eyes, gentle tiny smile, small red lips, peaceful expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary idle / waiting state with traditional chinese stationery,
seated at a traditional writing desk, ink brush placed neatly on brush rest,
inkstone, ink stick, rice paper, paperweight arranged neatly on desk,
both hands resting gently on the desktop near the lower frame,
body facing forward, head upright, calm relaxed seated posture,
breathing softly, waiting quietly, peaceful and elegant,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, holding tools, active writing pose, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.12 document_secretary.error（笔杆塞嘴里 · 头顶冒问号）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large confused dark eyes looking upward, eyebrows slightly raised, tiny brush handle between lips, thoughtful puzzled expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary stuck thinking state with traditional chinese stationery,
seated at a traditional writing desk,
right hand holding a calligraphy brush, the brush handle gently held between her lips like a thinking gesture,
left hand resting on the desk, fingers lightly tapping or touching the rice paper,
inkstone, ink stick and partially written rice paper on the desk,
eyes looking upward toward the ceiling or into space, pondering deeply,
question mark symbols floating above her head, one large and a few small,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, angry expression, frustrated expression, crying, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, brush in mouth like a cigar, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.13 document_secretary.done（放下笔 · 满意微笑）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large happy squinting dark eyes, warm contented cute smile, small red lips curved upward,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary task complete state with traditional chinese stationery,
seated at a traditional writing desk, task just finished,
calligraphy brush placed down on brush rest, both hands now relaxed and withdrawn from writing,
one hand resting on the desk, the other hand lightly on her lap or desk edge,
body relaxing back slightly after completing the work,
inkstone, ink stick, and finished rice paper with abstract flowing ink strokes on the desk,
warm satisfied smile, job well done, peaceful and content,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, intense expression, wide open eyes, shouting, fist pump celebration, still holding brush, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.14 document_secretary.shy（脸红害羞）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large shy dark eyes looking away or down to the side, bright pink blush on cheeks and nose, small shy pouty red lips, embarrassed expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary shy / embarrassed state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
calligraphy brush placed on brush rest on the desk, NOT held in hands,
both hands resting gently on the desk or one hand lightly touching her cheek,
eyes looking away or downward, avoiding eye contact, shy embarrassed demeanor,
cheeks and nose visibly flushed with pink warm blush, slightly tilted head,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, angry expression, crying, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.15 document_secretary.shy_peek（捂脸偷看）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large shy dark eyes peeking through slightly parted fingers, bright pink blush spreading across exposed cheeks, embarrassed but curious peek-a-boo expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary shy / hiding face state with traditional chinese stationery,
seated at a traditional writing desk,
both hands raised to cover the lower half of the face in a classic anime shy gesture,
fingers slightly apart, eyes clearly visible peeking through the finger gaps, looking at the camera or person,
cheeks and exposed skin visibly flushed with pink blush, embarrassed but curious,
ink brush, inkstone, rice paper and paperweight on the desk behind her hands,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, hands covering eyes completely, hands covering entire face with no visible eyes, angry expression, crying, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.16 document_secretary.tired（慵懒打哈欠）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with one eye squeezed shut tightly in a yawn, the other eye half-lidded and sleepy, a tiny glistening tear droplet at the corner of the closed eye, small red lips opened wide in a deep yawn, drowsy languid expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary tired / sleepy state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
one hand (palm open) raised loosely covering the mouth mid-yawn, fingers slightly apart and relaxed, not covering the entire face,
the other hand resting naturally on the desktop near the paper,
one eye squeezed shut with a tiny tear at the corner, the other eye half-closed and looking downward, drowsy and languid posture, elegant even when yawning,
ink brush placed on brush rest, no active writing, lazy break time,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, hand fully covering face, both hands on face, mouth hidden, crying, watering eyes, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.17 document_secretary.drink（喝奶茶）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with large sparkling dark eyes looking directly at the camera, cute content expression, small red lips wrapped around a straw gently,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary drinking bubble tea at a traditional writing desk,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight still arranged neatly in the background,
holding a chinese-style ceramic bubble tea cup with both hands near her chest, traditional celadon or blue-and-white porcelain tea cup shape but larger with a sealed film lid,
a pastel-colored wide straw inserted through the lid into the cup, the straw tip gently held between her lips,
eyes looking directly at the camera with a cute satisfied expression, gentle moe feeling,
ink brush placed on brush rest, small inkstone and rice paper visible behind the cup on the desk,
playful contrast between traditional desk props and modern bubble tea,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, different outfit, standing pose, walking pose, no desk, modern plastic cup, disposable cup, takeaway cup, paper cup, straw in eye, water splashing, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.18 programmer.嫌弃（动漫式脸黑 · 嫌弃表情）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with a mild annoyed half-lidded side-eye gaze, eyeballs slightly rolled to one side in subtle displeasure, small red lips pressed into a slight flat line, faint subtle shadow across the upper half of the face (light anime-style dramatic shadow, NOT fully blackened face, just a gentle shadow gradient suggesting an unimpressed mood), mild disdainful expression without being overly angry,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer slight disdain / unimpressed state at a modern workstation,
sitting in an ergonomic chair leaning back slightly, arms loosely crossed in front of chest or one elbow resting on chair armrest with hand dangling relaxed,
looking at the viewer (or the camera) with a mildly unimpressed side-eye gaze, subtle shadow across upper face hinting at displeasure, slight pout,
computer monitor, compact keyboard and mouse on desk in front of her, monitor shows abstract code or blank screen, no readable text,
mildly annoyed but elegant, dismissive in a tsundere-like way rather than genuinely angry, cool detached vibe,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, completely blacked out face, horror expression, angry shouting, open mouth yelling, crying, happy expression, big smile, laughing, different outfit, standing pose, walking pose, no desk, missing workstation desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, sofa, bed, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.19 programmer.整理（嘴里叼橡皮筋 · 双手整理头发）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with calm relaxed eyes looking slightly upward or to the side while fixing hair, NOT looking at camera, small red lips gently holding a black elastic hair tie between the teeth, casual composed expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

programmer tidying hair state at a modern workstation,
sitting in an ergonomic chair, both arms raised behind the head, both hands reaching back to gather or adjust her hair at the back of the head,
a small black elastic hair tie gently held between her teeth, lips slightly parted holding the band,
head tilted slightly forward or to the side naturally as she works on her hair,
eyes looking away from the camera — looking downward or off to one side, not engaging with the viewer,
computer monitor, compact keyboard and mouse on desk in front of her, monitor shows abstract code or blank screen, no readable text,
casual relaxed break moment, tidying up after a long coding session, elegant even during a casual grooming gesture,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette, navy and red accent contrast,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, looking at camera, eyes directly facing camera, both hands holding phone, both hands holding brush, hands covering face, elastic band fully inside mouth, biting elastic, chewing, eating, tongue visible, messy hair covering entire face, angry expression, different outfit, standing pose, walking pose, no desk, missing workstation desk, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

#### 12.14.20 document_secretary.tired_shy（打哈欠被抓 · 脸红害羞）

```text
1girl, solo, live2d desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi (roughly 1/3 of total body height),
round cute face with one eye squeezed shut shyly in the middle of a yawn, the other eye half-lidded and looking away embarrassed, a tiny glistening tear droplet at the corner of the closed eye, small red lips opened in a yawn but quickly pulling back into a shy pout, bright pink blush across cheeks and nose, caught-off-guard flustered expression,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with pearl hairpins, loose face-framing strands,
wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape,
5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons,
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

document secretary caught yawning / shy state with traditional chinese stationery,
seated at a traditional writing desk with ink brush, inkstone, ink stick, rice paper and paperweight arranged neatly,
one hand quickly covering the mouth mid-yawn in a flustered gesture after being caught, fingertips touching the cheek,
the other hand resting on the desktop or fidgeting with the edge of the rice paper,
one eye squeezed shut with a tiny tear, the other eye darting away embarrassed,
cheeks and nose flushed bright pink, caught off guard while yawning, shy flustered demeanor,
ink brush placed on brush rest, no active writing, flustered break moment,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,

NEGATIVE: full chibi super deformed proportions, tiny stubby limbs, oversized head taking half body height, realistic photorealistic face, mature adult face, angry expression, crying real tears, hands fully covering face, both hands on face, mouth completely hidden, missing smartwatch, missing bracelet, missing red bead buttons, traditional knotted buttons, laptop, keyboard, modern pen, readable text, watermark, logo, distorted hands, extra fingers, colored background, shadow on background, floor shadow
```

---

### 12.15 统一 Negative Prompt

> 适用于除 `programmer.agent_leader` 以外的桌前工作子形象；`programmer.agent_leader` 使用其 prompt 内单独的 Negative Prompt。

```text
different character, different outfit, casual clothing, office suit, hoodie, school uniform,
standing pose, walking pose, full body standing, no desk, missing desk, floating pose,
black hair, blonde hair, short hair, twin tails,
missing pearl hairpins, tassels, dangling pearl strands, pearl necklace-like hair ornaments,
missing smartwatch, missing bracelet, missing red bead buttons,
traditional knotted buttons, butterfly knot buttons, fabric knot buttons,
wrong qipao color, long sleeves, non-qipao dress,
busy background, readable text, logo, watermark, signature,
distorted hands, extra fingers, colored background, grey background, checkerboard background, fake transparent background, scenic background, busy background, shadow on background, floor shadow, fused fingers, hidden wrists, bad anatomy,
multiple characters, character clones, duplicate body, split panels,
brand logos, Claude logo, Codex logo
```

---

*归档日期：2026-05-19（v3 冻结版），2026-06-04 增补 12.16 节。*

---

### 12.16 桌宠静态 PNG 动态化专用生图 Prompt（2026-06-04 增补）

> **来源**：综合 12.14 「Q版脸 + 正式版身材」变体（综合观感最好的混合风格），增补桌宠静态 PNG 动态化项目所需的全部生图任务。
>
> **画布规格**：
> - 主形象 320×420 px（角色高度 90%，脚底贴 y≈380）
> - 关系图标 32×32 px（实际图案 ≤ 24×24）
> - 仪式插画 1280×800 px（背景）；装饰条 320×80 px
>
> **格式**：PNG-24 + Alpha 通道，sRGB，< 200KB/张
>
> **基础风格**：`semi-chibi proportions, head roughly 1/3 of total body height, round cute face + realistic adult female body`
>
> **眼位约束（关键）**：所有主图的眼睛中心必须落在 `y=22%`、眼高 `7%`（用于前端 CSS mask 眨眼）。`--eye-y: 22%; --eye-h: 7%;`

#### 12.16.0 统一角色块 + 统一 Negative（所有主图共享）

```text
1girl, solo, desktop pet sprite, upper body bust shot from waist up,
semi-chibi proportions, head slightly larger than realistic but NOT full chibi
(roughly 1/3 of total body height),
round cute face with large dark expressive eyes, small nose, tiny red lips,
gentle soft expression, eyes centered at 22% from top, eye height 7% of total,
but body retains realistic adult female proportions, slender arms, detailed elegant hands,

young cute chinese woman, medium-length brown hair, half-up hairstyle with
pearl hairpins, loose face-framing strands,
[QIPAO_VARIANT]    ← 见 12.16.1 三选一
pearl drop earrings, black square smartwatch with black strap on LEFT wrist,
double-layer beaded bracelet on RIGHT wrist, thin silver ring on right hand,
dark teal / muted cyan nail polish clearly visible,

anime key visual style, clean line art, soft cel shading, highly detailed, official art style,
solid pure white background, no shadows, no gradients, isolated character and props, clean silhouette,
soft overcast lighting, muted color palette,
masterpiece, best quality, 8k, ultra detailed,
```

统一 Negative（叠加在 12.15 之上，主图专用）：

```text
full chibi super deformed proportions, tiny stubby limbs, oversized head
taking half body height, realistic photorealistic face, mature adult face,
sharp angular jawline, eyes outside y=22% region,
different outfit, casual clothing, hoodie, school uniform, office suit,
standing pose, walking pose, full body standing, floating pose,
black hair, blonde hair, short hair, twin tails,
missing pearl hairpins, tassels, missing smartwatch, missing bracelet,
missing red bead buttons, traditional knotted buttons, butterfly knot buttons,
busy background, readable text, logo, watermark, signature,
distorted hands, extra fingers, fused fingers, hidden wrists, bad anatomy,
colored background, grey background, checkerboard background, fake transparent background,
scenic background, shadow on background, floor shadow,
multiple characters, character clones, duplicate body, split panels
```

#### 12.16.1 旗袍颜色三选一

| 变体 ID | 用途 | prompt 片段（替换 [QIPAO_VARIANT]） |
|---|---|---|
| `QIPAO_NAVY` | 默认主推（文书） | `wearing dark navy blue sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, matte fabric with good drape, 5 red round bead buttons on diagonal placket, small red round beads, NOT traditional knotted buttons` |
| `QIPAO_IVORY` | 程序员（工作站） | `wearing ivory white sleeveless modified qipao dress with subtle dark floral jacquard texture, traditional mandarin collar, bodycon fit, silk-like sheen, 5 silver round bead buttons on diagonal placket, side slit` |
| `QIPAO_BURGUNDY` | 旗袍 / 新中式（P2） | `wearing burgundy red sleeveless modified qipao dress, traditional mandarin collar, bodycon fit, satin fabric with gentle sheen, 5 gold round bead buttons on diagonal placket, side slit` |

#### 12.16.2 主形象 · 清和 · 文书（6 张，P0 必需）

> 服饰：`QIPAO_NAVY`；场景：传统书写桌（砚台、毛笔、米字格宣纸、镇纸）
> 共用：12.16.0 统一角色块 + 12.16.1 选 NAVY + 12.16.0 统一 Negative
> 仅描述「姿态差异」追加在统一角色块之后。

| # | 文件名 | 对应归档 | 姿态差异 prompt |
|---|---|---|---|
| 1 | `qinghe/normal.png` | 12.14.11 | `seated at a traditional writing desk, both hands resting naturally on the desk near rice paper, calm idle posture, eyes soft and gazing forward, gentle tiny smile` |
| 2 | `qinghe/thinking.png` | 12.14.3 | `seated at the writing desk, both hands supporting her cheeks, elbows resting on the desk, eyes looking directly at the camera with playful thoughtful expression, a calligraphy brush placed on brush rest on the desk, NOT held in hands or mouth` |
| 3 | `qinghe/sleepy.png` | 12.14.16 | `seated at the writing desk, one eye squeezed shut mid-yawn with a tiny tear droplet at the corner, the other half-closed and drowsy, one hand loosely covering the mouth, the other hand resting on the desk, brush placed on brush rest, languid break time` |
| 4 | `qinghe/writing.png` | 12.14.4 | `seated at the writing desk, right hand sweeping a calligraphy brush in bold flowing arc, left hand pressing the rice paper steady, confident upright posture, abstract flowing ink strokes on paper with no readable text` |
| 5 | `qinghe/error.png` | 12.14.12 | `seated at the writing desk, holding a calligraphy brush vertically in her mouth like a thinking gesture, a small abstract question mark floating above the head, eyebrows furrowed slightly, looking at the rice paper with puzzled expression` |
| 6 | `qinghe/done.png` | 12.14.13 | `seated at the writing desk, brush placed down on brush rest, both hands lightly clasped before chest, eyes gently closed with satisfied smile, small abstract check mark floating beside the rice paper, peaceful completed expression` |

#### 12.16.3 主形象 · 清和 · 程序员（5 张，P0 必需）

> 服饰：`QIPAO_IVORY`；场景：现代工作站（人体工学椅、显示器、键盘、鼠标）
> 显示器内容用"abstract code blocks / patch diff shapes"代替，禁止可读文字。

| # | 文件名 | 对应归档 | 姿态差异 prompt |
|---|---|---|---|
| 1 | `programmer/normal.png` | 12.14.11 改造 | `seated in an ergonomic chair at a modern workstation, both hands resting naturally on the desk near a compact keyboard, computer monitor behind showing abstract blurred code blocks with no readable text, calm ready posture` |
| 2 | `programmer/thinking.png` | 12.14.5 | `seated in an ergonomic chair leaning far back comfortably, both hands clasped behind the head with elbows spread outward, computer monitor showing abstract blurred code, lazy thoughtful gaze looking slightly upward` |
| 3 | `programmer/sleepy.png` | 12.14.16 改造 | `seated at the workstation, one hand loosely covering a yawn, eyelids drooping, other hand resting on the desk near the keyboard, monitor dimly lit and showing abstract dimmed code, drowsy at the keyboard` |
| 4 | `programmer/coding.png` | 12.14.7 | `seated in an ergonomic chair leaning forward close to the monitor, both hands typing rapidly on a compact keyboard with mouse beside, monitor showing abstract code blocks and patch diff shapes, focused and efficient posture` |
| 5 | `programmer/done.png` | 12.14.9 | `seated in the chair leaning back slightly, right hand raised with index finger placed vertically at the center of her lips in a quiet "shh" gesture, left hand resting on the armrest, half-lidded confident gaze at the viewer, monitor showing abstract green check visual` |

#### 12.16.4 主形象 · 清和 · 旗袍 / 新中式（5 张，P2 可选）

> 服饰：`QIPAO_BURGUNDY`；场景：依活动选择（无固定工作台，立姿居多）

| # | 文件名 | 对应归档 | 姿态差异 prompt |
|---|---|---|---|
| 1 | `qipao/normal.png` | 12.14.1 改立姿 | `standing upright with elegant posture, body facing slightly 3/4 angle, head upright, hands lightly clasped before waist, calm poised expression, no props` |
| 2 | `qipao/thinking.png` | 12.14.3 改立姿 | `standing with one hand holding a closed folding fan near the chin, the other hand on the waist, playful thoughtful expression, head tilted 3 degrees` |
| 3 | `qipao/sleepy.png` | 12.14.16 改立姿 | `standing with one hand lightly covering a yawn, shoulders dropped, head slightly tilted, fan held loosely in the other hand at her side` |
| 4 | `qipao/leading.png` | 12.14.6 改造 | `standing upright in a ceremonial posture, right hand raised and pointing forward in a presenting gesture, left hand at the waist holding a closed fan, dignified but warm expression` |
| 5 | `qipao/done.png` | 12.14.13 改立姿 | `standing in a slight bow of thanks, hands lightly clasped before chest, eyes gently closed, satisfied graceful smile` |

#### 12.16.5 关系阶段图标（5 张，P1 必需）

> 画布 32×32 px；线稿为主；纯白背景；同一系列统一 1.5px 描边
> 颜色按段位渐变：stranger 灰 → close_friend 知性紫 + 金

| # | 文件名 | prompt |
|---|---|---|
| 1 | `relationship/stranger.png` | `icon, single small empty circle outline, thin 1.5px stroke, color #9ca3af (cool gray), centered on pure white background, simple clean line art, no fill, no decoration, 32x32 canvas, no text` |
| 2 | `relationship/initial.png` | `icon, single small flower bud with one leaf, line art with soft fill, color #a5b4fc (pale lavender), centered on pure white background, 32x32 canvas, gentle cute style, no text` |
| 3 | `relationship/colleague.png` | `icon, single small branch with two leaves, line art with soft fill, color #818cf8 (indigo), centered on pure white background, 32x32 canvas, neat clean style, no text` |
| 4 | `relationship/friend.png` | `icon, two overlapping flower petals forming a small blossom, line art with soft fill, color #a78bfa (violet), centered on pure white background, 32x32 canvas, friendly warm style, no text` |
| 5 | `relationship/close_friend.png` | `icon, small blooming flower with one tiny glowing highlight dot above, line art with soft fill, color #c4b5fd (lavender) with single #fcd34d (gold) highlight, centered on pure white background, 32x32 canvas, intimate warm style, no text` |

> 统一 Negative（适用全部关系图标）：
> `filled background, dark background, multiple icons, complex detail,
> realistic photo, 3D render, text, watermark, distorted shapes`

#### 12.16.6 仪式感插画（4 张，P1 可选；多数可改用 SVG 兜底）

| # | 文件名 | 画布 | prompt |
|---|---|---|---|
| 1 | `ceremony/welcome.png` | 1280×800 | `background illustration, soft warm light circular gradient centered, delicate ring of small floating light particles, muted cream and pale lavender palette, anime background art style, peaceful inviting atmosphere, no characters, suitable for text overlay` |
| 2 | `ceremony/stage_up.png` | 1280×800 | `background illustration, soft radial light burst from upper center, scattered delicate petal shapes and small star sparkles floating outward, pale indigo and gold palette, anime background art style, ceremonial celebratory atmosphere, no characters, suitable for text overlay` |
| 3 | `ceremony/festival_lantern.png` | 320×80 | `icon strip, row of three small chinese lanterns hanging from a thin horizontal line, red and gold color palette, gentle swaying implied, transparent background, 320x80 canvas, line art with soft fill, no text` |
| 4 | `ceremony/heart_floating.png` | 60×60 | `icon sprite sheet, three small heart shapes of varying sizes (12px, 16px, 20px) arranged in a small cluster, soft pink #f9a8d4 to lavender #c4b5fd gradient, transparent background, 60x60 canvas, no text` |

> 统一 Negative（适用全部仪式插画）：
> `characters, people, text, watermark, busy composition,
> dark background, harsh colors, realistic photo`

#### 12.16.7 一致性约束（重要，生成后人工自检）

1. **姿态间对位**：同一系列 normal/thinking/sleepy/error/done 必须**几乎同一张图**，
   差异 ≤ 1-2mm，避免 CSS 切换时头部跳动
2. **眼位精确**：`y=22%`、`h=7%`（CSS mask blink 用此参数）；眼睛中心误差 ≤ 2px
3. **脚底锚点**：所有主图 transform-origin 在 y=92%（即 `50% 92%`）；角色重心靠下
4. **头部位置偏差**：同一系列各姿态头部中心误差 ≤ 4px
5. **服装不变**：同一 avatar 跨姿态的服装、发型、配饰完全一致
6. **PNG 自检脚本**：
   ```bash
   for f in qinghe/*.png programmer/*.png qipao/*.png; do
     convert "$f" -trim +repage -bordercolor none -border 1 "${f%.png}_safe.png"
   done
   ```
   应得到带 1px 安全边的透明 PNG；可肉眼对比正常描边

#### 12.16.8 目录与命名

```
apps/desktop/src-tauri/resources/avatar/
├── qinghe/         ← QIPAO_NAVY,    6 张 (P0)
├── programmer/     ← QIPAO_IVORY,   5 张 (P0)
├── qipao/          ← QIPAO_BURGUNDY,5 张 (P2)
├── relationship/   ← 32×32 关系图标, 5 张 (P1)
└── ceremony/       ← 仪式感插画,    4 张 (P1, 可全 SVG 替代)
```

#### 12.16.9 验收口径

- 5 张姿态图（normal/thinking/sleepy/error/done）叠在一起能重合 ≥ 95%
- `--eye-y=22%`、`--eye-h=7%` 的位置画在眼睛中央
- 透明背景无杂边（自检脚本通过）
- 文件大小 < 200KB/张
- 缩到 50% 后表情依然可辨
- 桌面 320×420 实际显示尺寸，角色居中、脚底贴 y=380

---

*本节内容专为桌宠静态 PNG 动态化项目（docs/桌宠静态PNG动态化与前端综合优化方案-20260604.md）准备；与 Live2D rigging 解耦，仅作静态 PNG 资产。*
