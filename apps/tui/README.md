# readio

<sub>These are engineering notes, in Chinese. For what readio is and how to use it, see the [repository README](../../README.md) — English, with a Chinese translation alongside.</sub>

一个终端阅读器，交互语法照着 coding agent 做：**你按回车，它「思考」、发起一次工具调用、然后把书里的文字流式吐出来。**

界面上看到的每一个数字都是真的——真实的段落偏移、真实的行号区间、真实的全文检索命中数。伪装的只是叙事外壳，不是数据。

## 安装 / 分发

```bash
# 装好就能用，不需要 Rust 工具链
curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh

# 或者自己编译（需要 Rust 1.85+）
cargo install --git https://github.com/hrhrng/readio readio
```

装的东西只有一个：`~/.local/bin/readio`，3.9MB，不带模型、不带资源、没有运行时依赖。要卸载就删掉这个文件，再删 `~/.readio`。

分发方式选得很朴素，理由是这个项目的形状决定的：

| 方式 | 用不用 | 为什么 |
| --- | --- | --- |
| 预编译二进制 + `install.sh` | **主路径** | 读者要的是一个能立刻跑的阅读器，不是一次十分钟的编译 |
| `cargo install --git` | 备用 | 给已经有 Rust 的人；也是 Windows 目前唯一的路 |
| Homebrew tap | 以后 | 等有人真的要之前，维护一个 tap 的成本大于收益 |
| crates.io | 以后 | `Cargo.toml` 的元信息已经填好了，发布只差一条命令 |
| 打包进容器 / 系统包 | 不做 | 一个读书的 TUI 装在容器里没有意义 |

### 为什么能这么简单

依赖树里**没有 C 代码**。原来 `zip` 默认带 `zstd-sys` 和 bzip2，需要 C 编译器；EPUB 只用 store 和 deflate，所以把它裁成 `default-features = false, features = ["deflate"]` 之后整棵树变成纯 Rust。直接的好处是 musl 静态编译只是加一个 target，不用 cross 工具链，一个 Linux 产物在任何发行版上都能跑；顺带二进制从 5.4MB 掉到 3.9MB（`lto = "thin"` + `strip` 又省了一截）。

四个产物：`aarch64-apple-darwin`、`x86_64-apple-darwin`、`x86_64-unknown-linux-musl`、`aarch64-unknown-linux-musl`。打一个 `tui-v*` tag 就由仓库根目录的 `.github/workflows/tui-release.yml` 全部构建、算 `SHA256SUMS`、传上去；CI 是同一层的 `tui-ci.yml`，只在 `apps/tui/**` 变动时触发。

版本走 beta 通道：tag 形如 `tui-v0.2.0-beta.N`，带 `-beta` 的 tag 在 release 里自动标成 prerelease。因此 `install.sh` 用的是 releases 列表而不是 `/releases/latest`——后者会跳过 prerelease，在只有 beta 的阶段等于什么都装不上。东西还在被用出问题的阶段，不值得为每次修一个 bug 就往上抬一个稳定版号。

### install.sh 做和不做的事

做：认出系统和架构、下载对应产物、**用 release 里的 `SHA256SUMS` 校验**、解包、`mv` 原子替换（所以升级时正在运行的 readio 不受影响）、装进 `~/.local/bin`、发现不在 `PATH` 里就告诉你怎么加。

不做：不用 sudo、不写 `/usr/local`、不改你的 shell 配置文件、不往 `~/.readio` 之外留任何东西、失败时不留半个二进制。校验失败直接拒绝安装并把两个哈希都打出来。

它是 POSIX `sh`，因为装东西的脚本不该要求你先有一个特定的 shell：

```bash
sh scripts/install.sh --version tui-v0.2.0-beta.1  # 指定版本
sh scripts/install.sh --dir /usr/local/bin  # 换目录（写权限自己准备）
```

写完之后我用一个本地 HTTP server 假装成 release 真跑了一遍——`READIO_BASE_URL` / `READIO_API_URL` 就是为此留的口子（它们只影响这个脚本，readio 本身仍然不读环境变量）。跑出来两个真 bug：POSIX `sh` 没有局部变量，`verify()` 里的 `archive="$1"` 覆盖了全局的同名变量，`tar` 于是收到一条重复拼接的路径；`tar -xzf FILE -C DIR` 的参数顺序在 BSD tar 上也不对。现在四条路径都验过：正常安装、哈希不匹配（拒绝且不留文件）、产物不存在（报清楚是哪个 target）、重复安装（原子替换，无临时目录残留）。

## 两条入口

**一、导入并开始读。** 默认把文件复制一份进书库目录：

```bash
readio book.epub        # 等同 -c：复制一份到 ~/.readio/books
readio book.epub -c     # 复制：原文件留在原处
readio book.epub -l     # 引用：不复制，只记一条指向原路径的记录
readio book.epub -m     # 移动：搬进书库，原文件不再保留
```

支持 `.epub` / `.pdf`（文字型）/ `.txt` / `.md`。书里的插图会用半块字符直接画在终端里。

**二、进书库挑。** 不带参数启动，列出所有导入过的书：

```
 readio   书库                                                        书库 3 本

 ◈ 书库  3 本
   ○  1. 注意力的形状                              1.3k字   30%  copy
   ○  2. 从 grok-build 抽壳的可行性结论            3.5k字    3%  link
   ○  3. 移动测试                                  3.6k字    0%  move

   回车继续上次的《注意力的形状》（第 1 本），或输入别的序号。
```

输入序号就开始读，回车直接续上次那本。

## 读起来是什么样

```
 readio   注意力的形状  readio 示例                          ch 2/4  ·  30.1%

 ❙ Thought for 0.3s

 ● Read epub://注意力的形状/OEBPS/ch02.xhtml  L14-28  ·  0.2s
   # 二 阅读的进度条
   进度条是一个奇怪的发明。它把一件本来连续的事切成了可以被丈量的段落。

   二 阅读的进度条

   进度条是一个奇怪的发明。它把一件本来连续的事切成了可以被丈量的段落。

   看电影时，进度条会告诉你还剩多久，于是你在最后十分钟总是紧张的。▌

 ✓ 已读 397 字  ·  5.7s
```

读完一章，它会真的把进度写下去，并且让你看见这次写盘：

```
 ✓ 本章读完：一 终端里的注意力  ·  392 字

 ● Write ~/.readio/state.json  ch1 complete  ·  0.1s
   { "chapter": 1, "para": 0, "progress": "30.1%" }
```

## 宿主目录

```
~/.readio/
  config.yaml     全部设置：语言、语速、朗读、插图
  library.json    导入过的书，按导入顺序
  state.json      阅读进度
  books/          -c 与 -m 导入的文件本体
  images/         从 EPUB 里抽出来的插图，按书归档
  speech/         朗读用的临时音频，播完即删
```

换目录：`readio --home /somewhere`。

界面默认英文（`language: en`），内置示例书也跟着界面语言走——英文界面配中文正文教不会任何人任何事。改成 `zh` 或者敲 `/lang zh` 就整套切过去，书的正文永远保持作者写的样子。

**readio 不读任何环境变量。** 所有设置都在 `config.yaml` 里，第一次启动时带注释写出来；`/speed`、`/voice`、`/rate`、`/lang`、`/tts` 这些命令会写回同一个文件。唯一不能放进去的是 `--home`——它决定这个文件在哪。

```yaml
language: en              # en 或 zh，默认 en
reading:
  speed: 46               # 每秒吐出多少字；朗读打开时按音频长度接管
  auto: false
images:
  enabled: true
  max_rows: 16            # 插图最高几行
tts:
  enabled: false
  engine: kokoro          # 见下面「朗读」
  voice: ""
  rate: 1.0
```

书的身份由**内容**决定（文件长度 + 前 128KiB 的哈希），不是路径。所以同一本书换个路径再导入不会变成两条记录，复制或移动进书库也不会丢掉之前读到哪。

## 交互

| 按键 | 作用 |
| --- | --- |
| `⏎` | 继续读下一段；有输入时按提问处理；书库里则打开上次那本 |
| `⏎`（输出中） | 加速当前这一轮，直接读到底 |
| `esc` | 打断输出 |
| `↑ ↓` / 滚轮 | 滚动，`pgup`/`pgdn` 翻页，`home`/`end` 到顶/底 |
| `^p` `^n` | 输入历史 |
| `^t` / `^o` | 折叠思考过程 / 工具调用 |
| `^g` / `^b` | 跳到下一处 / 上一处检索命中 |
| `^l` | 清屏 |
| `^c` `^d` | 退出（输出中时 `^c` 先打断） |

书库：`/lib` `/open <序号>` `/import <路径> [-c|-l|-m]` `/forget <序号>` `/sample`
阅读：`/toc` `/goto` `/next` `/prev` `/find` `/auto` `/plan` `/context` `/progress` `/speed`
朗读：`^s` 开关，`^r` 倍速，`/tts [on|off|<引擎>|test|config]` `/voice <音色>` `/rate <0.5-3.0>` `/device`
界面：`/lang zh|en`

## 朗读

readio **不带模型**，只调用你已经装好的引擎——所以二进制只有 2MB 出头，换成下个月更好的模型是改一行配置而不是等一个新版本。`config.yaml` 里预置了四个引擎模板（都来自 [tts-bench](https://github.com/5uck1ess/tts-bench) 2026 年 6 月那一轮的实测）：

| 引擎 | 大小 / 许可 | 为什么在这 |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | 默认。多语种（含中文），M4 上约 13.8× 实时，长文本质量最好 |
| `piper` | ~15M · GPL-3.0 | 最快：62ms 就出声、33.5× 实时。文本走 stdin |
| `supertonic` | 99M · MIT | 纯 ONNX，不依赖 torch，31 种语言 |
| `openai` | — | 任何 OpenAI 兼容的 `/v1/audio/speech` 服务（Kokoro-FastAPI 之类） |

模板就是命令行模板，占位符 `{text} {out} {voice} {rate} {model} {json} {file}`，所以 readio 没听说过的引擎也能接。句子永远作为**一个完整参数**传过去，不经过 shell——正文里出现 `$(whoami)` 也只是几个字。

朗读打开后两件事会变：

- **吐字速度跟着音频。** 每个片段的时长由音频文件自己报出来（`chars / clip_seconds`），所以文字会正好在声音停下时读完，`/speed` 让位给它。
- **两级高亮。** 正在念的那句浅底色，正在发声的那个词/字深底色加粗。中文按字走（没有空格可断，字也正是眼睛移动的单位），拉丁语按词走，标点跟着前一个字一起亮。

### 倍速与预取

`^r` 循环 0.75× → 1× → 1.25× → 1.5× → 2×（和网页播放器同一组档位，两个 app 手感一致），`/rate` 接受 0.5 到 3.0 的任意值，`/rate` 不带参数会把当前倍速和整条阶梯打出来。播放中倍速就显示在状态栏引擎旁边：`♪ kokoro · zf_xiaobei 1.5×`。

关键取舍是**倍速在合成时生效，不是播放时变速**——重采样会把音色一起改掉。代价是改倍速的那一刻，已经预取好的片段全都是旧速度的，必须扔掉。所以 `set_speed` 做三件事：作废整条流水线、记下当前正在念的那句的起点、从那一句重新排队。听感上就是「按一下，这句话重新用新速度念」，而不是等这一段读完。网页播放器改速时清空音频缓存是同一个道理。

预取本身是这次才真正接上的。原来 worker 是「合成一句 → 播一句 → 再合成下一句」的串行循环，`tts.prefetch` 这个配置项写进了文件、做了范围校验、注释也写好了，**但代码从来没读过它**——于是每个句子边界都有一个正好等于合成时长的空洞（kokoro 13.8× 实时，一句 5 秒的话就是 0.36 秒的静默）。现在是两个线程：

```
渲染线程 ──push──▶ 队列（容量 = prefetch）──pop──▶ 播放线程
                    ▲                                │
                    └── flush()：改速/打断时清空并删 wav ┘
```

队列用 `Mutex<VecDeque> + 两个 Condvar` 而不是有界 channel，原因很实际：有界 channel 没法从第三方清空——渲染线程会卡在「发送一个没人要的片段」上。带锁的队列让 `stop()` 能拿到锁、删掉临时 wav、同时唤醒两个线程。每个任务带一个 `era` 序号，`flush()` 递增它，两个线程都丢掉旧 era 的活儿，这样「作废」不需要杀线程。

三个测试钉住这套行为：下一句必须在当前句播完之前就渲染好（串行实现必然失败）、预取不许超出窗口（慢播放器背后不能堆一整章）、`stop()` 之后 scratch 目录里不许剩下 wav。

### 输出设备白名单

要防的事很具体：耳机断开或者休眠，系统悄悄把声音切回内置扬声器，下一句书就念给整个办公室听。白名单把「记得检查」变成程序的职责——只在你点名的设备上出声，切到别处就继续读、但不出声。

```yaml
tts:
  output:
    allow: ["AirPods", "bluetooth"]   # 名字的一部分即可，也可以写接口类型
    poll: 5                            # 每 5 秒检查一次
    on_mismatch: silence               # silence 静音（默认）| play 照念但提示
```

切到不在名单上的设备时，它不会默默静音——静音而不说明，和引擎坏掉是分不清的：

```
 ⚠ 朗读已静音：当前输出设备不在白名单
   现在的输出是「EarPods (usb)」，白名单里是：Studio Display
   怎么办：/device allow EarPods 把它加进白名单 · /device any 不再限制 · /device 看全部设备
```

`/device` 直接把设备列出来让你挑，`/device allow 1` 之类的立刻生效并写回配置；设备切回名单上的那台，朗读自己恢复：

```
 ⚠ 音频输出设备
    1. 杨的AirPods Pro #2 (bluetooth)   ← 当前  ✓ 白名单
    2. LS27D80xU (displayport)
    3. EarPods (usb)
    4. MacBook Pro扬声器 (builtin)
```

两个取舍写在代码里也值得写在这里：**认不出设备也算不在名单上**——宁可不响，也不要响错地方；**探测永远不在 UI 线程上**——问 macOS 当前输出设备要 200ms，够丢六帧，所以它跑在后台线程，界面读缓存。

直接输入一句话不是装样子：它会在全书做一次真实检索，用 `Grep` 的形态报出命中位置，再把最相关的几段引出来。

### 检索

对着 epy 核对过一遍，现在的行为和一个正经阅读器一致：

- **数全部命中，不是每段一次。** 早先的实现每段只取第一处、并且到上限就提前返回，于是「记忆」这个词在《看不见的城市》里 45 处会被报成 12 处。现在 `Book::search` 返回 `Hits { total, paragraphs, shown }`——表头上的 `45 matches · 30 lines · showing 12` 每个数字都是真数出来的。
- **折叠之后再比。** `fold()` 把大小写和全角（U+FF01..FF5E 减 0xFEE0、U+3000 → 空格）都归一化，同时为折叠后的每个字节记下原文的字节下标，所以高亮区间落回原文时不会错位。用英文键盘敲的词，在中文排版的正文里也能找到。
- **能跳过去，跳到还看得见。** 输序号、或者 `^g` `^b` 前后走；绕回开头会明说。落地时那个词在所属句子里被深高亮——句子由 `tts::sentence::split` 切出来，和朗读共用同一套两级高亮。
- **中文提问先收窄。** 一整句话当检索词几乎必然是 0 命中。`candidates()` 先剥掉「是什么样子」「怎么」「如何」这类疑问尾巴，再由宽到窄地试滑动窗口，返回书里真的出现过的最长短语。
- **不提供正则**——这一条是有意的：`/find ^第.章$` 这种东西会立刻把「coding agent」这层皮撕掉。

`/forget` 只删书库自己那份副本；`-l` 引用的原文件和 `-m` 移进来的文件都不会被删掉。

## 技术栈

和 Grok Build 同一条线，因为那套手感来自这些选择而不是别的：

| 层 | 用什么 |
| --- | --- |
| 渲染 | `ratatui` 0.29 |
| 终端 / 事件 | `crossterm` 0.28（alternate screen、鼠标、bracketed paste） |
| 事件循环 | `tokio` + `EventStream`，30fps 帧预算 |
| 文本 | `unicode-width` + `unicode-segmentation`（自写宽度感知折行） |
| 解析 | `zip` + `quick-xml`（EPUB）、`pdf-extract`（PDF） |
| 图片 | `image`（png/jpeg/gif/webp/bmp），半块字符 `▀` 渲染 |
| 存储 | `serde_json` + `serde_yaml_ng`，临时文件 + rename 落盘 |

## 结构

```
src/
  cli.rs          readio [文件] [-c|-l|-m] [--home <目录>] 的解析规则
  paths.rs        宿主目录、原子写
  config.rs       唯一的配置文件：语言、语速、插图、朗读引擎表
  i18n.rs         全部可见字符串，中英两列
  metrics.rs      伪装词汇：字数→token、进度→context、时长→本次时钟
  library.rs      导入（三种模式）、索引、按内容定身份
  store.rs        阅读进度
  wrap.rs         宽度感知折行：CJK 任意断行、避头尾、Latin 保词
  stream.rs       短语切分 + 字/秒配额的节流器
  book/           EPUB / PDF / 文本 → 章节与段落
    sample.rs     内置示例，中英各一篇，跟随界面语言
    epub.rs       container → OPF → spine → XHTML
    html.rs       宽容的 XHTML 提取（实体、未闭合标签、script/style、img）
    pdf.rs        文字型 PDF：按页取文，合并断行
    media.rs      从 EPUB 里抽插图（双重校验、防 zip-slip）
    text.rs       Markdown 与纯文本分章
  ui/
    block.rs      十种块：用户、思考、工具、正文、系统、事件、清单、上下文、插图、书库
    image.rs      半块字符渲染：一格两像素，长宽比按 1:2 校正
    scrollback.rs 条目列表 + 逐条行缓存 + 跟随尾部的视口 + 插图像素层
    prompt.rs     字素级光标的单行编辑器
    chrome.rs     顶栏、状态栏、帮助面板
  tts/
    config.rs     引擎模板与预置
    device.rs     输出设备白名单：探测、匹配、静音判定
    sentence.rs   分句，以及高亮走的词/字单元
    command.rs    调用外部引擎（参数注入安全）、播放、取消
    wav.rs        从音频文件本身读时长——吐字速度的来源
    mod.rs        后台合成/播放线程与事件
  app/
    turn.rs       回合状态机：思考 → 工具 → 流式 → 插图 → 事件
    flow.rs       阅读语义：把书的位置翻译成回合步骤
    mod.rs        状态、输入分派、布局
```

三条设计线值得单独说：

**回合是一个步骤队列。** `Turn` 只管时序——什么时候吐字、工具转多久、什么时候收尾；它完全不知道「书」这个概念。`flow` 负责把阅读位置翻译成步骤。所以换个内容源（RSS、论文、日志）只需要写一个新的 `flow`。

**节流器按字符预算放行。** 慢速时它会切开一个长短语只放行一部分，而不是攒够了一次性喷出来——这是「像在工作」和「像在刷屏」的分界线。

**身份来自内容。** 书库、进度、去重全部挂在内容哈希上，所以 `-c/-l/-m` 三种持有方式之间可以随时切换，读到哪一段都不会丢。

**插图是像素层，不是字符画。** 块只负责占位和标题行，像素在文字画完之后覆盖上去——所以它参与 ratatui 的差分、能被文字盖住、滚动时半截也能正确裁切。检测了 kitty / iTerm2 协议但目前统一走半块字符：它在任何终端都成立，没有「滚动之后图还挂在那」的问题。

## 测试

```bash
cargo test          # 211 个：折行、节流、解析、导入模式、阅读推进、整帧渲染、插图、朗读、设备白名单、检索跳转
```

- `tests/find.rs` 走「检索 → 输序号跳转 → 命中被深高亮」这条链，命中数按真实计数断言，`^g` 绕回来要说出来，没有检索时按 `^g` 要指回 `/find`。
- `tests/library.rs` 覆盖三种导入模式的文件系统后果（复制留原件、引用不复制、移动删原件）、重复导入不产生第二条记录、`/forget` 不误删用户自己的文件。
- `tests/render.rs` 用 ratatui 的 `TestBackend` 把真实 `App` 画出来再断言屏幕内容，所以「书库列出来了吗」「输入 2 打开第二本了吗」「esc 真的打断了吗」都有回归保护。
- `tests/audio_device.rs` 走一遍「白名单挡住当前设备 → 提示 → `/device allow` → 恢复」，`src/tts/device.rs` 里则用注入的探测函数模拟耳机休眠，断言判定只翻转一次（不会每帧刷提示）。
- `tests/illustration.rs` 现场生成 PNG 和 EPUB，一路验到「屏幕上真的有带颜色的半块字符」，没有会过期的固定素材。
- 所有测试跑在临时宿主目录里（`paths::set_home`），不会碰你真实的书库。

`scripts/pty_probe.py` 在真 pty 里跑二进制，带一个极小的终端模拟器把画面还原成文本——用来验证 raw mode、alternate screen、退出后终端是否复原这些只在真终端里才暴露的事。

```bash
python3 scripts/pty_probe.py 96 24 "wait:0.4,type:1,key:enter,wait:3" --home /tmp/readio-demo
```

CI 里也跑这一条（`.github/workflows/ci.yml`）：fmt、clippy `-D warnings`、全部测试，然后在真 pty 里开一次内置示例，断言 `[exit: clean]` 和 `[alt-screen restored: True]`——这两件事只有真终端会告诉你。

它还会解析 SGR 背景色，末尾打一张高亮图——`-` 是正在念的句子，`#` 是正在发声的那个字：

```
[highlight]  - sentence   # word
  13:    - - - - - - # # - - - - - - - - - - - - -
         界面不是中立的，它替你决定了什么值得注意。
```

调输入相关的问题时，在 `config.yaml` 里写 `input_log: /tmp/keys.log`，每个终端事件都会追加进那个文件。
