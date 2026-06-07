# NSIS 内测 + WiX MSI 正式发布 · 并行推进执行文档

> 日期: 2026-06-06
> 范围: 在保持 **同一份源码 / 同一份 `tauri.conf.json` 基线** 的前提下,产出两条独立分发轨道
> - 轨道 A: **NSIS 单 .exe 安装器** → 给内测用户(快迭代,不签名)
> - 轨道 B: **WiX MSI 安装包** → 给正式发布(签名、升级、修复语义)
> 决策原则: 共享底座最大化,差异只发生在"安装器壳"这一层
> 关联: [Graph快照-与最近工具调用-审查-20260606.md](./Graph快照-与最近工具调用-审查-20260606.md) §6 / [交接文档-20260606.md](./交接文档-20260606.md) / [Cairn架构对位与移植评估-20260604.md](./Cairn架构对位与移植评估-20260604.md)

---

## 1. 目标与边界

### 1.1 两条轨道的目标差

| 维度 | 轨道 A · NSIS (内测) | 轨道 B · WiX MSI (正式发布) |
|---|---|---|
| 用户群 | 内部 5-20 人,开发者 | 公开用户,合作方 |
| 迭代速度 | 周更甚至日更 | 月度稳定版 |
| 安装器签名 | 自签 / 不签 | Authenticode (EV/OV 证书) |
| 安装权限 | Per-User (`%LOCALAPPDATA%`) | Per-Machine (`%PROGRAMFILES%`) 或 Per-User |
| 升级语义 | 覆盖安装,版本号自管 | MSI UpgradeCode + Major Upgrade |
| 修复语义 | 不支持 | MSI Repair (`/f` 选项) |
| 多语言 | NSIS modern UI 多语 | WiX .wxl 本地化资源 |
| 体积 | 30-40 MB | 60-90 MB (LZX/Deflate 压缩) |
| CI 触发 | 每次 `main` push | 标签 `v*.*.*` push |
| 卸载入口 | 控制面板 / NSIS Add/Remove | 控制面板 / Programs & Features |
| 日志 | NSIS 安装日志(开箱) | WiX Burn bundle 日志 + MSI verbose log |

### 1.2 共享底座(两边都吃)

- 同一份 Rust workspace 源码 + 同一份前端 dist
- 同一份资源预处理流水线(PNG→WebP, MP4→WebM, 去 `*.cmo3`/`*.can3`/`hiyori_free_en.zip`)
- 同一份 `state-template/` + 干净 SQLite
- 同一份 `fastembed` 模型下载策略(首次启动拉取,不进安装器)
- 同一份 `cargo --release` 瘦二进制配置(`strip + LTO + opt-level=z`)
- 同一份 Tauri `frontendDist` 引用

### 1.3 不共享的部分(只发生在打包末段)

- `bundle.targets` (NSIS vs MSI)
- `bundle.nsis.*` vs `bundle.wix.*` 配置
- 签名证书路径 / 时间戳服务器
- 产物输出目录 + 产物命名规则
- 升级 GUID (MSI 专属)
- 一些 metadata(发行人、版权、help URL 等)

---

## 2. 共享底座实施(第 1-3 步,两边共做)

### 2.1 链接期瘦二进制 — Cargo 配置

`apps/desktop/src-tauri/Cargo.toml` 末尾追加:

```toml
[profile.release]
opt-level = "z"          # 体积优先
lto = "fat"              # 跨 crate LTO
codegen-units = 1        # 单 codegen unit,链接期更激进地内联
strip = "symbols"        # 删符号表
panic = "abort"          # 砍 unwinding 表
incremental = false
```

也加到 `crates/conductor-core/Cargo.toml`、`crates/conductor-cli/Cargo.toml`,确保三处 release 都一致。

预期: `conductor-desktop.exe` 30 MB → 12-15 MB,`conductor.exe` 25 MB → 8-10 MB。

### 2.2 资源预处理脚本 — `scripts/prepare-release-assets.ps1` (新)

```powershell
# 用法: pwsh scripts/prepare-release-assets.ps1 -DistDir apps/desktop/dist
param(
    [string]$DistDir = "apps/desktop/dist"
)
$ErrorActionPreference = "Stop"

# 1. 删冗余 — 不在运行时使用的源/重复文件
$toDelete = @(
    "live2d/hiyori/hiyori_free_en.zip",   # dist 里有 unzipped 副本
    "live2d/hiyori/hiyori_free_en/ReadMe.txt",
    "live2d/hiyori/hiyori_pro_en/ReadMe.txt"
)
foreach ($rel in $toDelete) {
    $p = Join-Path $DistDir $rel
    if (Test-Path $p) { Remove-Item -LiteralPath $p -Recurse -Force }
}
Get-ChildItem -Path $DistDir -Recurse -Include "*.cmo3","*.can3" -ErrorAction SilentlyContinue |
    Remove-Item -Force

# 2. PNG → WebP (lossless, z=9)
Get-ChildItem -Path $DistDir -Recurse -Include "*.png" | ForEach-Object {
    $out = $_.FullName -replace '\.png$', '.webp'
    & cwebp -z 9 -lossless $_.FullName -o $out
    if ($LASTEXITCODE -eq 0 -and (Test-Path $out)) { Remove-Item $_.FullName }
}

# 3. MP4 → WebM (VP9, CRF 40)
Get-ChildItem -Path $DistDir -Recurse -Include "*.mp4" | ForEach-Object {
    $out = $_.FullName -replace '\.mp4$', '.webm'
    & ffmpeg -y -i $_.FullName -c:v libvpx-vp9 -crf 40 -b:v 0 -row-mt 1 -an $out
    if ($LASTEXITCODE -eq 0 -and (Test-Path $out)) { Remove-Item $_.FullName }
}

# 4. 输出体积报告
$total = (Get-ChildItem -Path $DistDir -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("dist after preprocess: {0:N1} MB" -f ($total / 1MB)) -ForegroundColor Green
```

需要前端把 `apps/desktop/src/components/**/*` 和 `apps/desktop/src/windows/**/*` 里的 `.png` / `.mp4` 引用统一改成 `.webp` / `.webm`,加 `<picture>` 兼容回退(Tauri WebView2 = Chromium,WebP 100% 支持,严格意义上无须回退,但保守起见加上)。

### 2.3 把资源纳入 Tauri resources

[apps/desktop/src-tauri/tauri.conf.json](file:///I:/personal-agent/apps/desktop/src-tauri/tauri.conf.json) 把 `bundle.resources` 显式列出来,避免隐式打包:

```json
"bundle": {
  "active": false,                 // 留 false,只在 build 脚本里覆盖
  "targets": ["nsis"],
  "resources": [
    "resources/live2d/**",
    "resources/avatar/**",
    "../dist/index.html",
    "../dist/assets/**"
  ],
  "icon": ["icons/icon.ico"]
}
```

注: `active: false` 是因为我们要脚本按目标选择 nsis / msi,不在 conf 写死。

### 2.4 WebView2 引导策略

Tauri v2 NSIS / WiX bundler 都内置 WebView2 bootstrapper,默认行为是**阻塞引导**(Win10 1809 以下老机器)。

通用原则: **引导但不阻塞,后台静默下载**。两边用同一个值:

```json
"bundle": {
  "windows": {
    "webviewInstallMode": "downloadBootstrapper"
  }
}
```

`downloadBootstrapper` ≈ 1.7 MB 内嵌,运行时按需拉 WebView2 Evergreen,~70 MB 一次性下载后静默安装。

---

## 3. 轨道 A · NSIS 内测

### 3.1 配置文件拆分

新建 `apps/desktop/src-tauri/tauri.nsis.conf.json`,在原 conf 基础上叠加轨道 A 的设置:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "nsis": {
      "installerIcon": "icons/icon.ico",
      "installMode": "currentUser",     // 写 %LOCALAPPDATA%\Programs\PersonalConductor
      "languages": ["SimpChinese", "English"],
      "displayLanguageSelector": false,
      "installerHooks": null,           // 不写自定义 .nsh,等量齐观
      "compression": "lzma"             // NSIS 默认
    },
    "publisher": "Personal Conductor (Internal)",
    "shortDescription": "Personal Conductor 内测版",
    "longDescription": "内测版本,每周构建,无代码签名,可能存在不稳定行为。"
  }
}
```

合并策略:在 build 脚本里 `jq -s '.[0] * .[1]' tauri.conf.json tauri.nsis.conf.json > tauri.build.json`,再 `--config tauri.build.json` 传给 `cargo tauri build`。

### 3.2 启动器 .cmd 替换为 NSIS 安装

内测用户的引导入口从「解压 zip + 跑 启动.cmd」改为「双击 `Personal Conductor_0.1.0_x64-setup.exe`」。

NSIS 默认安装路径(`installMode: currentUser`):
- 程序目录: `%LOCALAPPDATA%\Programs\PersonalConductor\`
- 数据目录: `%LOCALAPPDATA%\PersonalConductor\`(由 conductor-core 自动派生 `Paths::data_dir()`)
- 卸载入口: 设置 → 应用 → 已安装的应用

需要在 `crates/conductor-core/src/paths.rs` 里把 `runtime_api_state_json` / `data_dir` / `config_path` 全部从 `exe 所在目录` 切到 `%LOCALAPPDATA%`,否则 Per-User 安装下 conductor.exe 写不到 Program Files。

**改造点**:
- `Paths::root()` 当前是 `current_exe().parent()`,改为 `%LOCALAPPDATA%\PersonalConductor`(WIN API `SHGetKnownFolderPath(FOLDERID_LocalAppData)`)
- `state-template/` 在安装时复制到 `%LOCALAPPDATA%\PersonalConductor\state\`
- `conductor-desktop.exe` 启动时检查 `state/config.json` 存在,缺则从 `state-template/` 复制(由 .cmd 改成在 main.rs 启动逻辑里做)

### 3.3 内测构建脚本 — `scripts/build-nsis.ps1` (新)

```powershell
# 用法: pwsh scripts/build-nsis.ps1 -Version 0.1.0 -Stamp 20260606
param(
    [string]$Version = "0.1.0",
    [string]$Stamp = (Get-Date -Format "yyyyMMdd-HHmm")
)
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $Root "apps\desktop"
$TauriDir = Join-Path $DesktopDir "src-tauri"

# 1. 资源预处理
& pwsh "$Root/scripts/prepare-release-assets.ps1" -DistDir "$DesktopDir/dist"

# 2. 前端构建
& npm.cmd run build --prefix $DesktopDir

# 3. 干净 DB
$CleanRoot = "$Root/release/_build/clean-root-release"
if (Test-Path $CleanRoot) { Remove-Item $CleanRoot -Recurse -Force }
New-Item -ItemType Directory -Path $CleanRoot -Force | Out-Null
$env:CONDUCTOR_ROOT = $CleanRoot
& cargo build --release -p conductor-cli
& "$Root/target/release/conductor.exe" proposal list
Remove-Item Env:CONDUCTOR_ROOT

# 4. 合并 conf
$mergedConf = "$TauriDir/tauri.build.json"
& jq -s '.[0] * .[1]' "$TauriDir/tauri.conf.json" "$TauriDir/tauri.nsis.conf.json" | Out-File -Encoding utf8 $mergedConf

# 5. Tauri build (只出 nsis)
$env:TAURI_CONFIG = $mergedConf
& cargo tauri build --bundles nsis

# 6. 移动产物
$nsisOut = "$TauriDir/target/release/bundle/nsis"
$artifacts = Get-ChildItem -Path $nsisOut -Filter "*.exe"
foreach ($a in $artifacts) {
    $dst = "$Root/release/artifacts/Personal-Conductor-nsis-v$Version-$Stamp.exe"
    New-Item -ItemType Directory -Path (Split-Path $dst) -Force | Out-Null
    Copy-Item $a.FullName $dst -Force
    Write-Host "NSIS: $dst ($([math]::Round($a.Length/1MB,1)) MB)" -ForegroundColor Green
}
```

### 3.4 内测分发渠道

| 渠道 | 方式 | 触发 |
|---|---|---|
| 飞书群 | 上传到飞书云空间,发链接 | 手动 / 每周一 cron |
| 内网 SMB | `\\corp\software\PersonalConductor\internal\` | 自动同步 |
| GitHub Releases (prerelease 标记) | `gh release upload` | 手动打 tag |

**不建议**走正式分发(Steam / 微软商店 / winget),这些是轨道 B 的目标。

---

## 4. 轨道 B · WiX MSI 正式发布

### 4.1 WiX 工具链准备

Tauri v2 的 WiX bundler 内置 WiX 3.14+。需要的环境:

- **WiX 3.14 toolchain**: Tauri 在 `tauri-bundler` 里 bundle 了一份,不需要手动装
- **.NET 6 SDK** (可选,做 Burn bundle 时需要)
- **Windows SDK 10.0.19041+** 的 `signtool.exe`(签名用)
- **Authenticode 证书**: `.pfx` 文件 + 密码 + 时间戳 URL

### 4.2 配置文件拆分

新建 `apps/desktop/src-tauri/tauri.msi.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "active": true,
    "targets": ["msi", "nsis"],
    "wix": {
      "language": ["zh-CN", "en-US"],
      "templatePath": null,                // 走默认
      "fragmentPaths": [],
      "componentGroupRefs": [],
      "featureGroupRefs": [],
      "mergeModules": [],
      "enableMsiFeatures": ["InstallValidate"],
      "skipWebviewInstall": false
    },
    "publisher": "Personal Conductor Co., Ltd.",
    "shortDescription": "Personal Conductor 桌面 Agent 运行时",
    "longDescription": "Personal Conductor 是 Windows 桌宠形态的本地优先个人助理,支持 Goal 驱动的多 Agent 协作。",
    "homepage": "https://conductor.local/",
    "createUpdaterArtifacts": true
  }
}
```

### 4.3 升级 GUID 策略

WiX MSI 的 `UpgradeCode` **全产品生命周期固定**。我们在 `tauri.conf.json` 的 `identifier` 已经给了 `local.personal.conductor`,但 MSI 的 UpgradeCode 是 128-bit GUID 格式。Tauri 会自动从 identifier 派生一个稳定的 UpgradeCode,我们需要:

1. **第一次发布前**用 `uuidgen` 生成真正的 UpgradeCode,固定写进 `tauri.msi.conf.json`:

```json
"wix": {
  "upgradeCode": "{A3F1E9C7-2B4D-4E5F-9A8B-1C2D3E4F5A6B}"
}
```

2. **永远不要再改** UpgradeCode。改 = 卸载老版本装新版,产品身份变了。

3. **ProductCode** 跟随 `version` 变,这是 Tauri 默认行为,不用动。

### 4.4 签名流程

签名是 Per-Machine / EV 证书场景下必须的(否则 SmartScreen 直接拦截,普通用户装不上)。

**前置依赖**(`scripts/sign-msi.ps1`):

```powershell
param(
    [Parameter(Mandatory)] [string]$MsiPath,
    [Parameter(Mandatory)] [string]$CertPath,         # .pfx
    [Parameter(Mandatory)] [SecureString] $CertPassword,
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

# signtool 在 Windows SDK 里,例如:
# C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\signtool.exe
$signtool = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\signtool.exe"

& $signtool sign /f $CertPath /p $CertPassword `
    /fd SHA256 /tr $TimestampUrl /td SHA256 `
    /d "Personal Conductor Installer" `
    /du "https://conductor.local/" `
    $MsiPath

& $signtool verify /pa $MsiPath
if ($LASTEXITCODE -ne 0) { throw "Signature verification failed" }
```

证书安全: **`.pfx` 永远不入 git,不入 CI 日志**。CI 用 `GitHub Actions Encrypted Secrets` 或 `Azure Key Vault` 注入,本地构建用环境变量 `CONDUCTOR_CERT_PATH` + 交互式密码。

### 4.5 MSI 构建脚本 — `scripts/build-msi.ps1` (新)

```powershell
# 用法: pwsh scripts/build-msi.ps1 -Version 0.1.0 -Stamp 20260606 -CertPath "D:\certs\conductor.pfx"
param(
    [string]$Version = "0.1.0",
    [string]$Stamp = (Get-Date -Format "yyyyMMdd-HHmm"),
    [string]$CertPath = $env:CONDUCTOR_CERT_PATH
)
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $Root "apps\desktop"
$TauriDir = Join-Path $DesktopDir "src-tauri"

# 1-4. 与 build-nsis.ps1 共用(资源预处理 + 前端构建 + 干净 DB + 合并 conf)
# 这里不再重复,详见 4.6 的公共函数提取

# 5. Tauri build (只出 msi)
$env:TAURI_CONFIG = $TauriDir
& cargo tauri build --bundles msi

# 6. 签名
$msiOut = "$TauriDir/target/release/bundle/msi"
$msis = Get-ChildItem -Path $msiOut -Filter "*.msi"
if (-not $msis) { throw "No MSI produced" }

foreach ($m in $msis) {
    if ($CertPath -and (Test-Path $CertPath)) {
        $securePwd = Read-Host "Cert password" -AsSecureString
        & pwsh "$Root/scripts/sign-msi.ps1" -MsiPath $m.FullName -CertPath $CertPath -CertPassword $securePwd
    } else {
        Write-Warning "CertPath not set, MSI will be unsigned (do not distribute)"
    }

    $dst = "$Root/release/artifacts/Personal-Conductor-msi-v$Version-$Stamp.msi"
    New-Item -ItemType Directory -Path (Split-Path $dst) -Force | Out-Null
    Copy-Item $m.FullName $dst -Force
    Write-Host "MSI: $dst ($([math]::Round($m.Length/1MB,1)) MB)" -ForegroundColor Green
}
```

### 4.6 公共函数提取 — `scripts/_common-build.ps1` (新)

把 `build-nsis.ps1` / `build-msi.ps1` 共享的前 4 步抽出来:

```powershell
# scripts/_common-build.ps1
function Invoke-AssetPreprocess { param($Root, $DistDir) ... }
function Invoke-FrontendBuild    { param($DesktopDir) ... }
function Invoke-CleanDatabase    { param($Root, $CleanRoot) ... }
function Merge-TauriConfig       { param($TauriDir, $OverlayPath) ... }
```

`build-nsis.ps1` / `build-msi.ps1` 各 import 这一份,只覆盖自己的 `cargo tauri build` 部分。

### 4.7 正式发布渠道

| 渠道 | 方式 | 触发 |
|---|---|---|
| GitHub Releases | `gh release create v$VERSION` 上传 .msi | git tag `v*.*.*` 推送 |
| 微软商店 (winget) | `wingetcreate new` + 提交 PR 到 `microsoft/winget-pkgs` | 手动 / 季度 |
| 官网下载 | 静态文件托管(COS / S3) | 自动同步 |
| Steam (后续) | Steamworks SDK 打包 | 长期路线,不在本轮 |

---

## 5. 并行推进的工程实践

### 5.1 CI Matrix(`.github/workflows/release.yml`)

```yaml
name: release
on:
  push:
    tags: ['v*.*.*']           # 正式版触发
    branches: [main]           # 内测构建

jobs:
  build-nsis:
    if: github.ref == 'refs/heads/main'
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - uses: actions/setup-rust@v1
        with: { toolchain: stable }
      - uses: dtolnay/rust-toolchain@stable
      - name: Build NSIS
        shell: pwsh
        run: scripts/build-nsis.ps1
      - uses: actions/upload-artifact@v4
        with:
          name: nsis-installer
          path: release/artifacts/*.exe

  build-msi:
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - uses: actions/setup-rust@v1
      - name: Build MSI (unsigned)
        shell: pwsh
        run: scripts/build-msi.ps1
      - name: Sign MSI
        shell: pwsh
        env:
          CONDUCTOR_CERT_BASE64: ${{ secrets.CONDUCTOR_CERT_BASE64 }}
          CONDUCTOR_CERT_PASSWORD: ${{ secrets.CONDUCTOR_CERT_PASSWORD }}
        run: |
          $certBytes = [Convert]::FromBase64String($env:CONDUCTOR_CERT_BASE64)
          $certPath = "$env:RUNNER_TEMP/conductor.pfx"
          [IO.File]::WriteAllBytes($certPath, $certBytes)
          $secure = ConvertTo-SecureString $env:CONDUCTOR_CERT_PASSWORD -AsPlainText -Force
          scripts/sign-msi.ps1 -MsiPath (Get-ChildItem release/artifacts/*.msi).FullName -CertPath $certPath -CertPassword $secure
      - uses: softprops/action-gh-release@v2
        with:
          files: release/artifacts/*.msi
```

**触发矩阵**:
- 推 `main` → 跑 `build-nsis`,上 GitHub Actions artifacts(不发布)
- 推 `v*.*.*` → 跑 `build-msi` + 签名,发 GitHub Release

### 5.2 版本号语义

```
0.1.0-alpha.3   → 内测(NSIS)
0.1.0           → 正式(WiX MSI,固定 UpgradeCode)
0.1.1           → 正式补丁
1.0.0           → 正式大版本
```

`tauri.conf.json.version` 单一来源,两边共用。`ProductVersion` 字段给 MSI 用,`FileVersion` 字段给 NSIS 用,都从同一个 `version` 派生。

### 5.3 产物命名规则

```
release/artifacts/
├── Personal-Conductor-nsis-v0.1.0-20260606.exe       # NSIS,内测
├── Personal-Conductor-nsis-v0.1.0-20260607.exe       # 第二天
└── Personal-Conductor-msi-v0.1.0-20260606.msi        # MSI,正式
```

`Stamp` 永远是构建日期,内测允许同一天多产物,正式版每 tag 一产物。

### 5.4 数据迁移 / 配置兼容

两条轨道的数据目录统一到 `%LOCALAPPDATA%\PersonalConductor\`:

```
%LOCALAPPDATA%\PersonalConductor\
├── state\
│   ├── conductor.sqlite
│   ├── config.json
│   └── ...
├── models\         # fastembed 模型缓存
└── logs\
```

迁移场景:
- 内测版升级到正式版:目录完全兼容,正式版直接读取
- 正式版回滚到内测版:数据库 schema 兼容(走 SQLx migration),回滚 OK
- 跨设备迁移:整个文件夹打包,新设备解到同样位置即可

### 5.5 升级机制(轨道 B 专属)

MSI 的 Major Upgrade 走 WiX `MajorUpgrade` element,Tauri 默认生成。版本号规则:

| 旧版本 | 新版本 | MSI 行为 |
|---|---|---|
| 0.1.0 | 0.1.1 | 升级(同 ProductCode,新 ProductCode 由 Tauri 自动从 version 派生) |
| 0.1.0 | 0.2.0 | 升级(同 UpgradeCode,新 ProductCode) |
| 0.1.0 | 0.1.0 重打 | 警告但不阻止,装到同位置覆盖 |

NSIS 不做升级约束,后装覆盖前装。简单粗暴但够内测用。

---

## 6. 验收清单

### 6.1 体积验收

| 轨道 | 目标 | 实际 |
|---|---|---|
| NSIS (内测) | ≤ 40 MB | 测三次取平均 |
| MSI (正式) | ≤ 90 MB | 同上 |
| 解压后磁盘占用 | ≤ 130 MB | 装完查 `dir` |

### 6.2 安装验收(在干净 VM 上)

| 项 | 验证方法 |
|---|---|
| Win11 22H2 安装 | 双击 .exe,30-60s 装完,桌宠启动 |
| Win10 21H2 安装 | 同上,验证 WebView2 自动引导 |
| Win10 1809 安装 | 同上,验证 WebView2 引导页能完成 |
| 卸载 | 控制面板 → 卸载,验证 `%LOCALAPPDATA%\PersonalConductor` 是否保留(MSI 默认保留用户数据) |
| 修复(MSI 专属) | 控制面板 → 更改 → 修复,验证二进制被覆盖 |
| Per-Machine 升级(MSI 专属) | 0.1.0 → 0.1.1,验证文件被覆盖,数据保留 |

### 6.3 CI 验收

- [ ] `push main` → NSIS 构建在 6 分钟内完成(资源预处理 + 前端 + cargo + 链接期瘦二进制 + NSIS 打包)
- [ ] `push tag v*` → MSI 构建 + 签名 + 发布在 10 分钟内完成
- [ ] 失败 case(故意改坏 Cargo.toml)→ CI 红色,产物不出

### 6.4 业务验收(最小回归)

[交接文档-20260606.md](./交接文档-20260606.md) §2.1 列出的主链路 10 个落点,装完新安装器后,跑一遍冒烟:

1. 创建 Goal
2. 自动推进 OODA
3. 创建 AgentTask
4. chat executor 跑通
5. `goal_tasks` 置 `review_ready`
6. tick_reviewing 自动验收
7. Goal accepted

任何一个挂 = 退回到上一个 tag。

---

## 7. 风险与备选

| 风险 | 影响 | 应对 |
|---|---|---|
| NSIS 主题老旧、内测用户嫌弃 | 低 | 短期可忍;长期切 WiX |
| WiX 4 vs WiX 3 迁移 | 中 | 跟 Tauri 版本,目前 Tauri v2 用 WiX 3.14,稳定 |
| Authenticode 证书过期(通常 1-3 年) | 高 | CI 加监控,提前 30 天告警 |
| Tauri bundler 升级破坏 NSIS/MSI 配置 | 中 | conf 拆 overlay 而不是改主 conf,升级时只重测 overlay |
| 杀毒软件误报 NSIS 安装器 | 中 | 申请代码签名证书(也是 WiX 的前置),同步签 NSIS |
| WebView2 在中国 CDN 慢 | 中 | 内置 Evergreen Standalone Installer(约 70 MB)而不是 bootstrapper |
| 内测 / 正式数据混用 | 低 | `%LOCALAPPDATA%\PersonalConductor` 单一来源,无隔离问题 |

---

## 8. 落地时间表

| 阶段 | 工期 | 交付 |
|---|---|---|
| 第 1 周 | 资源预处理脚本 + Cargo 瘦配置 | `prepare-release-assets.ps1` + 资源体积降到 50 MB |
| 第 1 周 | NSIS conf 拆分 + `build-nsis.ps1` | 内测能出第一个 NSIS 安装器 |
| 第 2 周 | 内测版分发,收 5-10 人反馈 | 体积 / 安装体验定档 |
| 第 2 周 | WiX MSI conf 拆分 + `build-msi.ps1` | 第一个未签名 MSI 出来 |
| 第 3 周 | 申请证书 + 写签名脚本 | 第一个签名 MSI 出来 |
| 第 3 周 | CI matrix 配置 | main 触发 NSIS,tag 触发 MSI |
| 第 4 周 | 验收清单跑通 | 第一个正式版 `v0.1.0` MSI 发布 |

---

## 9. 关联文件清单(本轮要落地的)

| 文件 | 状态 | 用途 |
|---|---|---|
| `apps/desktop/src-tauri/Cargo.toml` | 改 | 加 `[profile.release]` |
| `crates/conductor-core/Cargo.toml` | 改 | 同上 |
| `crates/conductor-cli/Cargo.toml` | 改 | 同上 |
| `apps/desktop/src-tauri/tauri.conf.json` | 改 | `bundle.resources` 显式列出;`active: false` |
| `apps/desktop/src-tauri/tauri.nsis.conf.json` | 新 | NSIS 轨道覆盖层 |
| `apps/desktop/src-tauri/tauri.msi.conf.json` | 新 | WiX MSI 轨道覆盖层 |
| `scripts/prepare-release-assets.ps1` | 新 | 资源预处理(去重 + 重编码) |
| `scripts/_common-build.ps1` | 新 | 公共构建函数 |
| `scripts/build-nsis.ps1` | 新 | NSIS 构建 |
| `scripts/build-msi.ps1` | 新 | MSI 构建 |
| `scripts/sign-msi.ps1` | 新 | signtool 调用封装 |
| `crates/conductor-core/src/paths.rs` | 改 | `Paths::root()` 切到 `%LOCALAPPDATA%` |
| `apps/desktop/src-tauri/src/main.rs` | 改 | 启动时检查 state 目录 / 模板复制 |
| `apps/desktop/src/**/*.{ts,tsx}` | 改 | `.png`/`.mp4` 引用换 `.webp`/`.webm` |
| `.github/workflows/release.yml` | 新 | CI matrix(NSIS on main, MSI on tag) |

---

## 10. 一句话总结

> **同一份源码,同一份 Cargo 瘦配置,同一份资源预处理;只在 `cargo tauri build --bundles nsis|msi` 那一行分叉。** NSIS 给内测(快、不签名、Per-User),WiX MSI 给正式(慢、签名、Per-Machine、Major Upgrade)。所有差异通过 overlay conf + 不同 build 脚本承载,主 `tauri.conf.json` 保持稳定。
