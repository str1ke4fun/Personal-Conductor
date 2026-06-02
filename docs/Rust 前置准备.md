# Rust 前置准备 · Agent 替不了你的事

> 本文清单是**人工必做项**——这些事 vibe coding 里 agent 帮不上忙，必须你自己一次性搞定。
> 做完之后，你就只剩"测试→观错→架构层决策→派工"这条循环。
>
> 目标读者：**完全没装过 Rust** 的人。已经装过的，跳到 §6 自检即可。

---

## 0. 思路

你说自己的姿势是：

> 「装环境 + vibe coding：我做测试（使用）→ 观察错误 → 架构层决策 → 交给 agent 下一步」

这套姿势在 Rust 里**完全行得通**，但有 4 件事 agent 没法替你做：

1. **装工具链**（你机器上的环境，agent 看不到也改不了）。
2. **配 IDE 让你能看懂错误**（rustc 错误信息很长，没有 IDE 高亮 vibe 不起来）。
3. **跑通一个 hello world**（确认链路完整，否则后面任何报错都分不清是"你装错了"还是"agent 写错了"）。
4. **认识 5 个会反复出现的概念**（你需要在错误里认出它们，否则你给不出"架构层决策"）。

下面按顺序来。

---

## 1. 装 Rust 工具链（必做）

### 1.1 Windows 推荐路径：rustup-init

1. 去 https://rustup.rs/ 下载 `rustup-init.exe`，**双击运行**。
2. 它会先检测 MSVC C++ Build Tools。**如果没装**，它会让你先去装：
   - 装 **Visual Studio Build Tools 2022**（不是完整 VS，只要 Build Tools）。
   - 勾选 `Desktop development with C++`，里面只要 `MSVC v143` + `Windows 11 SDK` 两项即可。
   - 装完重启电脑（rustup 才能看到环境变量）。
3. 再跑 `rustup-init.exe`，默认选项一路回车。它会装：
   - `rustc`（编译器）
   - `cargo`（构建/包管理）
   - `rustup`（工具链管理）
   - 默认 channel：`stable`

### 1.1.1 关于 "我不想装 Visual Studio" 的替代方案

**关键澄清**：你不需要装 Visual Studio IDE。Rust 在 Windows 上需要的只是：
- **MSVC 编译器 + linker**（`cl.exe`, `link.exe`）
- **Windows SDK**（系统头文件和库）

这两个东西**单独打包**就叫 **Build Tools for Visual Studio 2022**，是个独立 installer，**不含 VS IDE 本体**。控制面板里它叫 "Visual Studio Build Tools 2022"，不叫 VS。

| 路线 | 装机大小 | MVP CLI | Round 2 Tauri 桌宠 | Round 3 Win32 hook / UIA | 推荐 |
|------|---------|---------|--------------------|--------------------------|------|
| A. MSVC Build Tools（独立 installer） | 3–4 GB | ✅ | ✅ 官方推荐 | ✅ `windows` crate 完美 | **首选** |
| B. GNU 工具链（`stable-x86_64-pc-windows-gnu`） | ~500 MB | ✅ | ⚠ 可能踩坑 | ❌ `windows` crate 残缺 | 不推荐 |
| C. LLVM/Clang 替换 MSVC | 视情况 | ⚠ | ⚠ | ⚠ | 不推荐 |

**为什么给你拍 A**：你这个项目终点是桌宠 + OS hook + UIAutomation，两件事都重度依赖 Win32 API。**用 MSVC 才不会在 Round 2/3 半路换工具链**。多花一晚下载，省后面所有折腾。

**省时小技巧**：

1. **下载先开始，去做别的事**——3–4 GB，最好挂着睡前下。
2. **可以装到非 C 盘**——installer 第一步选盘符，建议 `D:\BuildTools\` 或 `I:\BuildTools\`。
3. **装完不需要重启电脑**——新开一个 PowerShell 窗口就能跑 `rustup-init`。
4. **不要登录 Microsoft 账号**——点跳过。
5. **只勾 `Desktop development with C++`**——里面只保留 `MSVC v143` + `Windows 11 SDK`，其他全取消勾选。**千万别勾**："游戏开发用 C++"、"Linux/嵌入式"、"通用 Windows 平台"——每个都几 GB 没用。

**死活不想装 MSVC 的 plan B**：先用 GNU 跑通 Round 1。到 Round 2 接 Tauri 时再切：

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

代价：`target/` 全部重编一次。Round 1 代码本身不用动。

### 1.2 自检

新开一个 PowerShell：

```powershell
rustc --version    # 期望：rustc 1.78+ (xxx 2024-xx-xx)
cargo --version    # 期望：cargo 1.78+
rustup show        # 期望：active toolchain 是 stable-x86_64-pc-windows-msvc
```

三条都对 = 工具链 OK。

### 1.3 加组件（rustfmt / clippy）

```powershell
rustup component add rustfmt clippy
```

- `rustfmt`：保存时自动格式化（IDE 会调用）。
- `clippy`：静态检查工具，能在编译前抓出很多坏味道，**强烈建议从一开始就用**。

`rust-toolchain.toml` 已经在派工单 §2.2 写明会锁定这些组件，你装一次就好。

---

## 2. 装 IDE / 编辑器（必做）

### 2.1 推荐：VS Code + rust-analyzer

理由：你已经在用 VS Code（基于 Claude Code 的工作流），不用换。

1. VS Code 装扩展：
   - `rust-analyzer`（官方维护，**必须装**，这是看错误的命根子）。
   - `Even Better TOML`（看 Cargo.toml 高亮）。
   - `CodeLLDB`（如果你想 debug 单步；MVP 阶段可选）。

2. 第一次打开任何 Rust 项目时，`rust-analyzer` 会下载 sysroot 索引，**第一次大约 2–5 分钟**，耐心等。等完之后，错误会像 TypeScript 一样实时下划线。

3. **打开 `Settings → Editor → Format On Save`** 勾上——保存自动 rustfmt，避免风格漂移。

### 2.2 备选：RustRover

JetBrains 出的专门 Rust IDE，免费个人版可用。功能比 VS Code 强一点（重构更聪明），但你已经熟 VS Code，**没必要换**。

### 2.3 自检

打开任意 `.rs` 文件（哪怕是 `cargo new` 自动生成的 `main.rs`），故意写：

```rust
fn main() {
    let x: i32 = "hello";
}
```

保存。如果 IDE 立刻在 `"hello"` 下划红线，鼠标悬停能看到 `expected i32, found &str`——IDE OK。

如果没反应，说明 `rust-analyzer` 没装好或还在索引，等几分钟再试。

---

## 3. 跑一个 hello world（必做）

**目的**：确认"装环境 → 写代码 → 编译 → 运行"整条链路通了。如果这步过不去，后面派工都白搭。

```powershell
cd I:\
mkdir rust-hello
cd rust-hello
cargo init --bin
cargo run
```

期望输出：
```
   Compiling rust-hello v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.23s
     Running `target\debug\rust-hello.exe`
Hello, world!
```

看到 `Hello, world!` = **链路 OK，可以开 T1**。

跑通后这个目录可以删，它的作用只是验证环境。

---

## 4. 学会看错误 / 五个高频概念（必做）

vibe coding 在 Rust 里要成立，你不用会**写**很多代码，但要在 agent 给你写完后，**看一眼错误就能判断"这是哪类问题、该往哪个方向给 agent 提示"**。

下面 5 个概念会在你接下来 6 个工作日里反复出现。每个我给一行人话定义 + 报错长什么样 + 你该说什么。

### 4.1 Ownership（所有权）

**人话**：一个值同时只能有一个"主人"。把 `s` 传给函数，`s` 在调用方就用不了了。

**报错长这样**：
```
error[E0382]: borrow of moved value: `s`
```

**你该说什么**：
> "这里报 move 错误，能不能改成传引用 `&s` 或者 `.clone()`？"

### 4.2 Borrowing（借用）`&` / `&mut`

**人话**：`&T` 是只读借用，`&mut T` 是可变借用。**同一时刻要么多个 `&T`，要么一个 `&mut T`，不能混**。

**报错长这样**：
```
error[E0502]: cannot borrow `xxx` as mutable because it is also borrowed as immutable
```

**你该说什么**：
> "借用冲突，把那个不可变借用先用完再开始写，或者把数据 clone 一份"

### 4.3 Lifetime（生命周期）`'a`

**人话**：编译器要确认你的引用活得比它指向的数据短。

**报错长这样**：
```
error[E0106]: missing lifetime specifier
```

**你该说什么**：
> "加生命周期标注 `'a`，或者改成把数据所有权传进去（用 `String` 而不是 `&str`）"

### 4.4 `Result` / `?`

**人话**：Rust 没有异常。所有可能失败的函数返回 `Result<T, E>`。`?` 是简写：成功取出 T，失败立刻 return Err。

**报错长这样**：
```
error[E0277]: the `?` operator can only be used ... that implements `FromResidual`
```

**你该说什么**：
> "这个函数返回类型不是 Result，要么把它改成 `anyhow::Result<()>`，要么这里用 `.expect()` / `match`"

### 4.5 `async fn` / `.await`

**人话**：`async fn` 返回的不是值，是个 future，必须 `.await` 才会真的跑。

**报错长这样**：
```
warning: unused implementer of `Future` that must be used
```
或者：
```
error[E0728]: `await` is only allowed inside `async` functions
```

**你该说什么**：
> "这里漏了 `.await`" 或者 "调用方也得是 async，或者用 `tokio::runtime::Runtime::block_on`"

---

> ⚡ **真正的 vibe coding 姿势**：你不需要记住怎么修这些错。
> 你只需要**认出来"这是哪一类错"**，然后把错误信息原文 + 你的判断贴给 agent：
>
> > 「这报了 lifetime 错误，我倾向于把这里改成传 owned String 而不是 &str，你看怎么改最干净」
>
> 这就是"架构层决策"。Agent 会给你具体改法。

---

## 5. Windows / 中文环境特别项（必做一次）

### 5.1 控制台 UTF-8

Windows 默认控制台编码不是 UTF-8，跑 Rust 程序输出中文会乱码。

**一次性永久办法**：

控制面板 → 区域 → 管理 → 更改系统区域设置 → 勾选 `Beta: 使用 Unicode UTF-8 提供全球语言支持` → 重启。

**临时办法**（每次开 PowerShell 跑一下）：

```powershell
chcp 65001
```

派工单 §3.10 里讨论过两种处理方式，你选哪种就在前置阶段定好：
- A. 系统全局开 UTF-8（推荐，一次省心）
- B. 临时 chcp，每次启动 Conductor 终端跑一次
- C. 让程序自己调 `SetConsoleOutputCP(65001)`（依赖 `windows` crate，编译慢一点）

**给你拍**：选 **A**。一次设置，永远不用想。

### 5.2 路径分隔符

派工单里所有路径都用 `I:\personal-agent\...` 或 `I:/personal-agent/...`。Rust 标准库的 `PathBuf` 在 Windows 上两种都接受，你不用纠结。

### 5.3 长路径

如果你的家目录用户名是中文（看起来是「徐笙祺」），路径里会有中文，`cargo` 偶尔在某些第三方 crate 编译时出问题。**对策**：把 `I:\personal-agent\` 放在纯英文路径下（你已经是了，OK）。

但 Cargo 的全局缓存默认在 `C:\Users\<你的中文名>\.cargo\`，**建议改到纯英文目录**：

新建系统环境变量：
```
CARGO_HOME = D:\rust\.cargo
RUSTUP_HOME = D:\rust\.rustup
```

把现有 `C:\Users\...\.cargo` 和 `.rustup` 整个剪切到新位置，重启 PowerShell，跑 `cargo --version` 确认没坏。

**这一项做一次就行，受益终身**。

---

## 6. 自检清单（开 T1 前对一遍）

| # | 检查项 | 命令 | 期望 |
|---|--------|------|------|
| 1 | rustc 可用 | `rustc --version` | 1.78+ |
| 2 | cargo 可用 | `cargo --version` | 1.78+ |
| 3 | clippy 安装 | `cargo clippy --version` | 有输出 |
| 4 | rustfmt 安装 | `rustfmt --version` | 有输出 |
| 5 | VS Code rust-analyzer 工作 | 故意写错代码 | 实时红线 |
| 6 | hello world 跑通 | §3 步骤 | `Hello, world!` |
| 7 | 控制台中文不乱码 | `Write-Host "测试"` | 显示「测试」 |
| 8 | CARGO_HOME 在纯英文路径 | `echo $env:CARGO_HOME` | `D:\rust\.cargo` 或类似 |

**8 项全过 → 可以开 T1**。

---

## 7. 接下来的姿势（给你定一个工作循环）

```
            ┌──────────────────────────────┐
            │  你看派工单的一个 T（任务）   │
            └──────────────┬───────────────┘
                           │
            ┌──────────────▼───────────────┐
            │  把 T 的「做什么 + 验收」贴给  │
            │  agent，让它写 Rust 代码      │
            └──────────────┬───────────────┘
                           │
            ┌──────────────▼───────────────┐
            │  cargo build / cargo test     │
            │  你看输出                     │
            └──────────────┬───────────────┘
                           │
            ┌──────────────▼───────────────┐
            │  跑「验收」步骤               │
            └──────────────┬───────────────┘
                           │
              过？────┬────失败？
                     │         │
                     ▼         ▼
              进下一个 T   把错误 + 你的
                          架构层判断 贴回
                          给 agent
```

**你的核心动作只有 3 个**：

1. **派工**（贴文档某段给 agent）。
2. **跑验收**（命令在派工单里都写好了）。
3. **看错误说哪类问题**（§4 五个概念帮你认）。

写代码、调编译错、查 API 用法——**全是 agent 的活**。

---

## 8. 学到什么程度才"够用"

不用学完一本《Rust 程序设计语言》。**只要你能做到下面三件事，就够开 Round 1**：

1. 看到 `cargo build` 报错，能复制错误粘给 agent + 说一句"我猜是 XX 问题"。
2. 看到一段 Rust 代码，能识别"这是函数定义 / 这是 match / 这是 if let"，**不要求**理解每个符号。
3. 看到 agent 把一个函数签名改了，能问一句"为什么从 `&str` 改成 `String`？"，**不要求**自己能写出正确签名。

剩下的边干边补。**别在前置阶段一头扎进《The Rust Programming Language》读 200 页**，那样你三天没动 T1 就会失去耐心。

---

## 9. 推荐参考（按"用到再翻"原则）

| 场景 | 资源 |
|------|------|
| 找一个 API 怎么用 | https://docs.rs/ 直接搜 crate 名 |
| 编译错误代码（如 E0382）官方解释 | `rustc --explain E0382` |
| 看一段语法不认识 | https://cheats.rs/（速查表，单页） |
| 想系统补一次基础 | 《Rust 程序设计语言》中文版 https://kaisery.github.io/trpl-zh-cn/ 第 1-4 章 + 第 10 章 |
| async/tokio 怎么用 | https://tokio.rs/tokio/tutorial 中文版 https://rust-book.junmajinlong.com/tokio |

**不要从头读 trpl**。挑章节读。

---

## 10. 我什么时候来帮你

完成 §6 自检 8 项之后，你贴一句：

> 「Rust 环境就绪，开始 T1。」

我会：
1. 把 T1 的 `Cargo.toml` / `rust-toolchain.toml` 内容贴出来，让你创建文件。
2. 等你跑 `cargo build --workspace` 后贴输出给我，确认 workspace 联编成功。
3. 进 T2。

每个 T 完成后，你贴：
- `cargo build` 输出
- 验收步骤输出
- 任何"看起来怪"的代码片段

我做架构层判断 + 给下一步派工提示。

---

*配套：派工-Round1-MVP管道.md（开 T1 前请确认本文 §6 自检通过）。*
