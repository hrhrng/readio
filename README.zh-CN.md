# readio

**一个伪装成 AI coding agent 的开源终端电子书阅读器，也是本地 TTS 有声书播放器。** EPUB、PDF、Markdown 和纯文本都不用离开终端：按下回车，它会“思考”、发起一次工具调用，再把书里的下一段流式吐出来。看起来在 coding，其实在看书或听书。

[![tui-ci](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml/badge.svg?branch=main)](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml)
[![release](https://img.shields.io/github/v/release/hrhrng/readio?include_prereleases&filter=tui-v*&label=release&color=6f5ec7)](https://github.com/hrhrng/readio/releases)
[![license](https://img.shields.io/badge/license-MIT-6f5ec7)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.90%2B-6f5ec7)](https://www.rust-lang.org)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-6f5ec7)

[English](README.md)

用 Rust 写成，键盘优先。章节按书自己的目录来分，斜体保留，封面和插图通过终端原生图片协议精确渲染。朗读交给你自己选的本地模型，正在读的那句浅高亮、读到的那个字深高亮。核心是一个二进制，不带模型、不带资源、不要求运行时依赖或云端账号。

屏幕上的每一个数字都是真读出来的——真实的段落偏移、真实的行号区间、真实的全文检索命中数。伪装的只是词表，不是数据。

## 安装

> **readio 还在 beta。** 版本按 `tui-v0.Y.0-beta.N` 打标签，在 GitHub 上标为 prerelease，安装脚本取最新的那个。用的人多了才会稳下来——尤其是朗读，目前只对着命令模板和测试验过，并没有把表格里每个引擎都跑通。

```sh
curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh
```

Windows PowerShell：

```powershell
$installer = Join-Path $env:TEMP "readio-install.ps1"
Invoke-WebRequest https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.ps1 -OutFile $installer
& $installer
```

两个安装器都会下载对应的 release 产物，并用 release 里的 `SHA256SUMS` 校验。Unix 脚本安装到 `~/.local/bin/readio`，PowerShell 安装到 `%USERPROFILE%\.local\bin\readio.exe`。都不需要 sudo、编译器或 Rust，也不会改书库和配置。卸载程序只需删除这个可执行文件；只有确实想一并清除书籍、设置和进度时才删除 `~/.readio`。

从源码安装需要 Rust 1.90 及以上：

```sh
cargo install --git https://github.com/hrhrng/readio readio
```

预编译产物覆盖 macOS 的 `aarch64` / `x86_64`、Linux 的 `aarch64` / `x86_64`（musl 静态链接），以及 Windows x64。它们只是图方便，
不是唯一的路：BSD 或没人打包的架构仍可用上面那条命令自己编译，依赖树是纯 Rust，不需要 C 工具链。

每个 Pull Request 都会跑真实的五平台发布矩阵：在对应的原生 runner 上构建并启动可执行文件、生成最终归档，再汇总检查文件名和包内根目录结构。Pull Request 流程没有发布 GitHub Release 的权限。

## Windows 说明

Windows x64 有预编译 ZIP 和带 SHA-256 校验的 PowerShell 安装器。每个 Pull Request 都会在干净的 Windows runner 上构建并运行 exe、从伪造 release 安装、验证重复安装，并确认校验和错误时拒绝落盘。仍然可以从源码编译：

1. **装 Rust**，用 [rustup](https://rustup.rs)。默认的 `x86_64-pc-windows-msvc` 就好，它会提示你装 Visual Studio Build Tools（勾 *使用 C++ 的桌面开发*）。readio 里没有 C 代码，但 `rustc` 调用的链接器仍然是 MSVC 那一个。不想装 Visual Studio 就 `rustup default stable-x86_64-pc-windows-gnu`，配 MinGW-w64 也行。

2. **编译。** 在 PowerShell 里：

   ```powershell
   git clone https://github.com/hrhrng/readio
   cd readio\apps\tui
   cargo build --release
   .\target\release\readio.exe
   ```

3. **放进 PATH。** `cargo install --path .` 会把 `readio.exe` 装到 `%USERPROFILE%\.cargo\bin`，这个目录 rustup 已经加进 PATH 了。

4. **用支持 VT 的终端。** 想精确显示封面和插图，需要终端支持 Kitty graphics、iTerm2 inline images 或 Sixel；其他终端仍能运行 readio，但图片位置会显示占位框。如果只能用老的 `conhost`，先 `chcp 65001`，否则框线和中文会变成乱码。

5. **文件在哪儿。** `%USERPROFILE%\.readio` 下面是 `config.yaml`、`books\`、`state.json`。`readio --home D:\readio` 可以整体换个地方。

6. **朗读不用额外装播放器**：默认的 `play` 就是一条用 `Media.SoundPlayer` 的 PowerShell 命令。语音引擎仍然要你自己装，并把 `tts.engines.<名字>.synth` 改成它在 Windows 上的命令行。

POSIX 专用的 `scripts/pty_probe.py` 在 Windows 上跑不了。音频输出白名单在 Windows 上也没有内置的设备探测：把 `tts.output.query` 设成一条能打印当前输出设备名的命令（比如 PowerShell 加 `AudioDeviceCmdlets` 模块）；在你设好之前，`/device` 会说它读不到设备列表，并提醒白名单仍然让朗读保持静音。

Windows CI 会验证发布版可执行文件和安装器。部分本地语音模型的托管安装测试仍依赖 POSIX runtime；文本阅读和已打包的可执行文件不依赖这些路径。

## 用法

```sh
readio                 # 进书库
readio book.epub       # 导入并开始读（默认 -c 复制）
readio book.pdf -l     # 引用：只记路径，不复制
readio book.md -m      # 移动：把文件搬进书库
```

复制进来的书放在 `~/.readio/books`。`readio --home <dir>` 可以另开一个书库。手边没书就 `/sample`，有一篇内置短文。

**回车读下一段；斜杠命令操作这本书，`/find <词>` 才会检索全书。** 单独输入数字会选择屏幕上的书或搜索结果；其它文字留在输入框里，直到它成为一条命令，所以一个手误不会突然触发全书检索。

| 按键 | 作用 |
| --- | --- |
| `enter` | 继续或恢复；输出中加快当前一轮，朗读正文仍跟着人声 |
| `esc` | 中断并留在原处；`enter` 恢复 |
| `space` | 输入框为空时播放/暂停；朗读时精确保持音频指针 |
| `shift+tab` | 手动 → 自动 → 朗读循环 |
| `↑` `↓` · 滚轮 · `pgup` `pgdn` · `home` `end` | 滚动；手动模式在底部继续载入下一段 |
| `←` `→` | 朗读时上一句/下一句；其它时候移动输入光标 |
| `[` `]` | 朗读减速/加速 |
| `^t` · `^o` | 折叠/展开思考 · 工具调用 |
| `^s` | 进入朗读；再按一次回到之前的模式 |
| `^r` | 切换下一档推理强度；强度越高默认读得越慢 |
| `^g` · `^b` | 跳到下一处 · 上一处命中 |
| `^p` `^n` · `^l` | 输入历史 · 清屏 |
| `^c` · `^d` | 丢弃正在运行的一轮/确认退出 · 立即退出 |

| 命令 | 用途 |
| --- | --- |
| `/lib` `/open <n>` `/import <path>` `/forget <n>` `/sample` | 管理书库 |
| `/toc` `/plan` `/goto <n>` `/next` `/prev` | 选择章节或在章节间移动 |
| `/find <词>` | 全书检索；输序号跳到那一处 |
| `/mark [备注]` `/marks [序号]` `/unmark <序号>` | 记下这一处、列出、删掉 |
| `/mode [manual\|auto\|aloud]` `/auto` | 选择阅读模式 · 手动/自动切换 |
| `/effort [档位]` `/rate <0.5-3>` `/speed <n>` | 阅读强度 · 调当前档倍数 · 基础吐字速度 |
| `/voice`（`/tts` 别名）`/device` | 下载与配置声音 · 限制音频输出 |
| `/context` `/progress` | 本次阅读读数 · 一行位置摘要 |
| `/lang en\|zh` `/clear` `/help` `/quit` | 界面语言 · 清屏 · 帮助 · 退出 |

## 伪装成什么

| 阅读器里的概念 | 屏幕上的说法 |
| --- | --- |
| 读到全书的百分之几 | `ctx 23.3%`，context window 占用 |
| 一段有多少字 | `735 tok`，token 数 |
| 打开这本书多久了 | `0:15`，会话计时 |
| 取下一段正文 | 一次工具调用：`● Read book.epub#ch1  L1-9  ·  0.3s` |
| 全文检索 | 你发出的 `/find`，命中数是真的 |

## 按书本来的样子读

readio 跟的是文件里写的东西，不是文件的分法。

**章节来自目录。** 转换工具经常把十几章塞进一个 XHTML，再让目录指向文件内部的锚点；readio 就在那里切，所以目录列了 71 节的书就是 71 章，不是 13 章。spine 里标了 `linear="no"` 的文档（版权页、广告）不属于阅读顺序。目录没点名、自己也没有标题的那一段按序号称呼，而不是拿文件名当章名——`index_split_003` 说的是排版工具的事，跟这本书无关。

**斜体留住了。** 强调按范围叠在正文上，用终端的修饰位画出来；书里写的是 `<em>` 也好，是转换工具惯用的「CSS class 加 `font-style: italic`」也好，都认。它和朗读叠加：正在被读的斜体句子，既是斜体也带高亮。

**封面在你打开一本书时出现**，续读时不再出现。

**你记下的位置是真的记住了。** `/mark` 记下现在这一处，`/marks` 列出来，`/marks <序号>` 回去。书签和阅读进度都按字符偏移存，所以升级改变了分章方式之后，它们仍然指着同一句话——引入目录分章的那次升级把每个章号都挪了，没有人丢掉位置。

## 检索

`/find <词>` 会检索全书，并且数出**每一处**命中，而不是每段只算一次。表头先给出决定要不要看下去所需要的信息：`pattern: 记忆 · 45 matches · 30 lines · showing 12`。匹配时忽略大小写，全角标点和字母按对应的半角处理，所以用英文键盘敲出来的词照样能在中文排版的正文里找到。

输命中的序号就跳过去，`^g` `^b` 在列表里前后走，走到头会告诉你已经绕回来了。跳到之后，那个词在浅高亮的句子里被深高亮出来——和朗读用的是同一套两级高亮——眼睛落在词上，而不是落在一整段上。

也可以把自然语言问题明确交给 `/find`。中文提问本身很少正好是个检索词，所以 readio 会先收窄：先去掉“是什么样子”“怎么”这类疑问尾巴，再从剩下的文字里由宽到窄地试滑动窗口。返回的是书里真的出现过的最长短语。

## 朗读

readio 自己不带语音模型，新安装也默认不选择任何模型。`/voice` 打开一个明确分成两栏的工作台：左边只下载和校验模型，右边只把已经就绪的模型、音色、语种和参数配置到全局或明确选中的某本书。下载不会改配置，保存配置也不会偷偷下载。用 `shift+tab`、`^s` 或 `/mode aloud` 单独进入朗读模式。

| 引擎 | 体积 · 协议 | 说明 |
| --- | --- | --- |
| `moss` | 120M · Apache-2.0 | Apple Silicon 上推荐的中文有声书音色；常驻 |
| `kokoro` | 82M · Apache-2.0 | 推荐英文音色 `af_heart`；常驻 |
| `qwen` | 0.6B · Apache-2.0 | 可选中文替代；更大、声音也更硬 |
| `espeak` | 非神经网络 · GPL-3.0 | 多语种、瞬间出声但很机械；一个系统包 |
| `piper` | ~7–32M · GPL-3.0 | 音色决定中英文；神经音里出声很快 |
| `supertonic` | 99M · MIT | 多语种、英文最强；纯 ONNX，不依赖 torch |
| `openai` | 远端服务 | 任何兼容的 `/v1/audio/speech` 接口 |

下载模型前，readio 会先显示预计下载量和当前剩余空间，再请求确认。它会把固定版本的独立 `uv`、托管 Python、工具环境和启动器装进平台用户缓存，不要求电脑预装 Python、`uv`、`pip` 或 `pipx`；同时不覆盖普通的 uv 与 Hugging Face 缓存目录，所以用户已经下载过的内容仍然能直接命中。每条命令跑之前都写出来，跑的时候也看得见：

```
● Bash UV_TOOL_DIR=…/readio/runtime/tools …/readio/runtime/bin/uv
  tool install --managed-python --python 3.12 kokoro-tts  3/5  ·  24.8s
  Installed 61 packages in 3.42s
   + kokoro-tts==0.9.4

○ kokoro 下载完成，用了 25 秒。Voice 配置没有改变。
```

`openai` 是唯一的例外：那是你自己起的服务，readio 会直说装不了，而不是假装能装。`/tts` 只是打开同一个 Voice 工作台的兼容别名，不是另一套开关。

会多种语言的模型，仍然得有人告诉它现在看的是哪一种，而它的默认往往不是你要的那种：`kokoro-tts` 默认按 `en-us` 走，中文丢给它就是按英文规则发音。因此语种、引擎和音色都在 Voice 表单里明确配置为全局或 per-book 设置，正文绝不会逐句偷偷切换。

朗读时，正在读的那句浅高亮，正在发音的那个词或那个字深高亮；吐字速度跟着每段音频的真实时长走，不是估的。

倍速就是有声书那一套。朗读模式下，`[` 和 `]` 在 0.75×、1×、1.25×、1.5×、2× 之间减速/加速；`/rate` 可以把当前 effort 档位设为 0.5 到 3.0 的任意倍数，`^r` 则循环读和听共用的命名强度档位。内置播放器通过 libsonic 实时改变节奏并保持音调；标准 1× 音频、当前播放指针、合成缓存和已经预取的内容都不会因为变速失效。

空格会冻结当前 PCM 帧，再从同一个音频毫秒恢复。`←` 和 `→` 在不退出朗读模式的前提下跳到上一句或下一句。文字显现和两级高亮采样同一个播放时钟，所以暂停、跳句或变速都不会让文字自己跑远。

句子会在独立线程上提前合成。`voice.prefetch` 默认 8，是句数硬上限；范围之内的控制器会按实时倍速争取大约 24 秒播放余量，速度越快就适当多准备一些。段落之间也会预热下一段的第一句，而不是等声音停了再从头开始。

引擎能常驻的就让它常驻，不是每句话起一个进程——本地模型能不能用，多半就差在这里：同一句短句，Kokoro 走命令行要 8.4 秒，常驻并把 phonemizer 的后端缓存住之后是 0.5～0.8 秒。如果你更在意"马上出声"而不是"好听"，`espeak` 念同一句只要 0.03 秒，装它只需要一个 `brew` 或 `apt` 包。

朗读不会偷偷降级。声音断了——引擎没了、设备不在白名单、合成失败——阅读就停在声音停下的地方，并告诉你为什么，而不是继续往下滚给一个没在看屏幕的人。回车就是重试。

`/device` 把出声限制在指定的音频输出上。耳机断开、系统悄悄切回外放时，readio 会静音、说出它探测到的设备、并把解除办法留在屏幕上。认不出来的设备一律算不允许。

## 配置

只有一个文件 `~/.readio/config.yaml`，首次运行时带注释生成。readio 不读任何环境变量。界面默认英文，`language: zh` 换成中文。`/mode`、`/effort`、`/speed`、`/voice`、`/rate`、`/lang`、`/device` 这些命令会把改动写回同一个文件。

## 开发

```sh
cd apps/tui
cargo test                                     # 完整 Rust 测试套件
python3 scripts/pty_probe.py 96 24 "wait:0.6,type:/sample,key:enter,wait:2"
```

这个探针在真实 pty 里驱动二进制，把它画出的那一屏打印出来，包括哪些格子被高亮了。这里真正会出问题的是 raw mode、alternate screen 和退出时终端有没有还原——单元测试看不到这些。CI 在 macOS 和 Linux 上跑全套测试，两边都跑探针。

## 仓库结构

`apps/tui` 是这个阅读器；[`apps/tui/README.md`](apps/tui/README.md) 是它的实现说明——模块地图、各层守住的不变量、怎么测、怎么发（英文）。

`apps/web`、`apps/api`、`apps/extension` 是这个仓库最初的 Speechify-like 网页栈，安装说明在 [`docs/web-api-extension.md`](docs/web-api-extension.md)。

## 协议

MIT，整个仓库都是。见 [`LICENSE`](LICENSE)。
