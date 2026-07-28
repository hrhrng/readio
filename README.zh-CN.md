# readio

**一个终端阅读器，交互语法照着 coding agent 做。** 你按回车，它「思考」、发起一次工具调用、然后把书里的下一段流式吐出来。

[![tui-ci](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml/badge.svg?branch=main)](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml)
[![release](https://img.shields.io/github/v/release/hrhrng/readio?include_prereleases&filter=tui-v*&label=release&color=6f5ec7)](https://github.com/hrhrng/readio/releases)
[![license](https://img.shields.io/badge/license-MIT-6f5ec7)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.85%2B-6f5ec7)](https://www.rust-lang.org)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-6f5ec7)

[English](README.md)

支持 EPUB、文字型 PDF、Markdown 和纯文本。章节按书自己的目录来分，斜体保留，封面和插图直接画在终端里。朗读交给你自己选的本地模型，正在读的那句浅高亮、读到的那个字深高亮。一个 4 MB 的二进制，不带模型、不带资源、没有运行时依赖，也不读任何环境变量。

屏幕上的每一个数字都是真读出来的——真实的段落偏移、真实的行号区间、真实的全文检索命中数。伪装的只是词表，不是数据。

```
 readio   The Shape of Attention  readio sample                   ch 1/4  ·  ctx 0.0%
   ○  3. Three  The feel of a tool  783 tok
   ○  4. Four  One continuous piece of work  485 tok

 ❙ Thought for 1.3s

 ● Read readio://sample/attention.md  L1-9  ·  0.3s
   # One  Attention in eighty columns
   The first time I noticed that attention has a shape, I was watching a cursor…
   I was waiting on a slow build. There was nothing on the screen but that one …

   One  Attention in eighty columns

   The first time I noticed that attention has a shap▌
╭────────────────────────────────────────────────────────────────────────────────────╮
│❯ enter to keep reading, or ask a question / type a command                         │
╰────────────────────────────────────────────────────────────────────────────────────╯
  ⠼ One  Attention in eighty co…  ·  esc to stop  ·  ↑↓ … ⸬ readio-1  64 tok  ·  0:06
```

<sub>这一屏由 `apps/tui/scripts/pty_probe.py` 从一个真实 pty 抓下来，不是手写的。</sub>

## 安装

> **readio 还在 beta。** 版本按 `tui-v0.Y.0-beta.N` 打标签，在 GitHub 上标为 prerelease，安装脚本取最新的那个。用的人多了才会稳下来——尤其是朗读，目前只对着命令模板和测试验过，并没有把表格里每个引擎都跑通。

```sh
curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh
```

脚本会识别系统和架构、下载对应产物、用 release 里的 `SHA256SUMS` 校验，然后把一个文件装到 `~/.local/bin/readio`。不需要 sudo、不需要编译器、不需要 Rust 工具链，也不会在安装目录之外留下任何东西。卸载就是删掉这个文件，再删 `~/.readio`。

从源码安装需要 Rust 1.85 及以上：

```sh
cargo install --git https://github.com/hrhrng/readio readio
```

预编译产物覆盖 macOS 的 `aarch64` / `x86_64` 和 Linux 的 `aarch64` / `x86_64`（musl 静态链接）。它们只是图方便，
不是唯一的路：其他平台——Windows、BSD、没人打包的架构——用上面那条命令自己编译就行，依赖树是纯 Rust，不需要 C 工具链。
Windows 的准确说法是没测过，而不是不支持。

## 在 Windows 上自己编译

Windows 没有预编译产物，也没有安装脚本——准确的说法是没测过，而不是不支持。依赖树是纯 Rust，所以编译只需要 Rust 和一个链接器，别的都不用。

1. **装 Rust**，用 [rustup](https://rustup.rs)。默认的 `x86_64-pc-windows-msvc` 就好，它会提示你装 Visual Studio Build Tools（勾 *使用 C++ 的桌面开发*）。readio 里没有 C 代码，但 `rustc` 调用的链接器仍然是 MSVC 那一个。不想装 Visual Studio 就 `rustup default stable-x86_64-pc-windows-gnu`，配 MinGW-w64 也行。

2. **编译。** 在 PowerShell 里：

   ```powershell
   git clone https://github.com/hrhrng/readio
   cd readio\apps\tui
   cargo build --release
   .\target\release\readio.exe
   ```

3. **放进 PATH。** `cargo install --path .` 会把 `readio.exe` 装到 `%USERPROFILE%\.cargo\bin`，这个目录 rustup 已经加进 PATH 了。

4. **用支持 VT 的终端。** readio 需要真彩色、alternate screen，以及用半块字符画插图，Windows Terminal 三样都支持。如果只能用老的 `conhost`，先 `chcp 65001`，否则框线和中文会变成乱码。

5. **文件在哪儿。** `%USERPROFILE%\.readio` 下面是 `config.yaml`、`books\`、`state.json`。`readio --home D:\readio` 可以整体换个地方。

6. **朗读不用额外装播放器**：默认的 `play` 就是一条用 `Media.SoundPlayer` 的 PowerShell 命令。语音引擎仍然要你自己装，并把 `tts.engines.<名字>.synth` 改成它在 Windows 上的命令行。

有两样东西跟不过来：`scripts/install.sh` 是 POSIX sh，`scripts/pty_probe.py` 需要 POSIX pty，在这儿都跑不了——直接编译、直接运行就好。音频输出白名单在 Windows 上也没有内置的设备探测：把 `tts.output.query` 设成一条能打印当前输出设备名的命令（比如 PowerShell 加 `AudioDeviceCmdlets` 模块）；在你设好之前，`/device` 会说它读不到设备列表，并提醒白名单仍然让朗读保持静音。

`cargo test` 应该能跑——整帧渲染测试走的是 ratatui 的 `TestBackend`，不需要真终端——但没人在 Windows 上跑过全套，所以那里挂了算 bug，欢迎报。

## 用法

```sh
readio                 # 进书库
readio book.epub       # 导入并开始读（默认 -c 复制）
readio book.pdf -l     # 引用：只记路径，不复制
readio book.md -m      # 移动：把文件搬进书库
```

复制进来的书放在 `~/.readio/books`。`readio --home <dir>` 可以另开一个书库。手边没书就 `/sample`，有一篇内置短文。

**回车读下一段；输入的任何文字都会被当成提问，去做全文检索。**

| 按键 | 作用 |
| --- | --- |
| `enter` | 继续读；输出中再按一次直接冲到底 |
| `esc` | 打断 |
| `↑` `↓` · 滚轮 · `pgup` `pgdn` · `home` `end` | 滚动 |
| `^t` · `^o` | 折叠/展开思考 · 工具调用 |
| `^s` · `^r` | 开关朗读 · 切换倍速（0.75× → 2×） |
| `^g` · `^b` | 跳到下一处 · 上一处命中 |
| `^p` `^n` · `^l` · `^c` `^d` | 输入历史 · 清屏 · 退出 |

| 命令 | 用途 |
| --- | --- |
| `/lib` `/open <n>` `/import <path>` `/forget <n>` | 管理书库 |
| `/toc` `/goto <n>` `/next` `/prev` | 章节间移动 |
| `/find <词>` | 全书检索；输序号跳到那一处 |
| `/mark [备注]` `/marks [序号]` `/unmark <序号>` | 记下这一处、列出、删掉 |
| `/auto` `/speed <n>` | 自动续读 · 吐字速度 |
| `/context` `/progress` `/plan` | 现在读到哪儿 |
| `/tts [引擎]` `/voice [auto|zh|en|<名字>]` `/rate <0.5-3>` `/device` | 用哪个声音念、装引擎、音频输出 |
| `/lang en\|zh` `/help` `/quit` | 界面语言 · 帮助 · 退出 |

## 伪装成什么

| 阅读器里的概念 | 屏幕上的说法 |
| --- | --- |
| 读到全书的百分之几 | `ctx 23.3%`，context window 占用 |
| 一段有多少字 | `735 tok`，token 数 |
| 打开这本书多久了 | `0:15`，会话计时 |
| 取下一段正文 | 一次工具调用：`● Read book.epub#ch1  L1-9  ·  0.3s` |
| 全文检索 | 你提的那个问题，命中数是真的 |

## 按书本来的样子读

readio 跟的是文件里写的东西，不是文件的分法。

**章节来自目录。** 转换工具经常把十几章塞进一个 XHTML，再让目录指向文件内部的锚点；readio 就在那里切，所以目录列了 71 节的书就是 71 章，不是 13 章。spine 里标了 `linear="no"` 的文档（版权页、广告）不属于阅读顺序。目录没点名、自己也没有标题的那一段按序号称呼，而不是拿文件名当章名——`index_split_003` 说的是排版工具的事，跟这本书无关。

**斜体留住了。** 强调按范围叠在正文上，用终端的修饰位画出来；书里写的是 `<em>` 也好，是转换工具惯用的「CSS class 加 `font-style: italic`」也好，都认。它和朗读叠加：正在被读的斜体句子，既是斜体也带高亮。

**封面在你打开一本书时出现**，续读时不再出现。

**你记下的位置是真的记住了。** `/mark` 记下现在这一处，`/marks` 列出来，`/marks <序号>` 回去。书签和阅读进度都按字符偏移存，所以升级改变了分章方式之后，它们仍然指着同一句话——引入目录分章的那次升级把每个章号都挪了，没有人丢掉位置。

## 检索

`/find <词>`——或者直接在提示符下问一句话——会检索全书，并且数出**每一处**命中，而不是每段只算一次。表头先给出决定要不要看下去所需要的信息：`pattern: 记忆 · 45 matches · 30 lines · showing 12`。匹配时忽略大小写，全角标点和字母按对应的半角处理，所以用英文键盘敲出来的词照样能在中文排版的正文里找到。

输命中的序号就跳过去，`^g` `^b` 在列表里前后走，走到头会告诉你已经绕回来了。跳到之后，那个词在浅高亮的句子里被深高亮出来——和朗读用的是同一套两级高亮——眼睛落在词上，而不是落在一整段上。

中文提问本身很少正好是个检索词，所以 readio 会先收窄：先去掉"是什么样子""怎么"这类疑问尾巴，再从剩下的文字里由宽到窄地试滑动窗口。返回的，是书里真的出现过的那个最长短语的检索结果。

## 朗读

readio 自己不带语音模型，只按配置文件里的命令模板去调用你装好的引擎，所以换模型是改一行配置，而不是等一个新版本。

| 引擎 | 体积 · 协议 | 说明 |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | 默认；多语种，长文本质量最好 |
| `piper` | ~15M · GPL-3.0 | 出声最快；文本走 stdin |
| `supertonic` | 99M · MIT | 纯 ONNX，不依赖 torch，31 种语言 |
| `openai` | — | 任何 OpenAI 兼容的 `/v1/audio/speech` 服务 |

`/tts` 问的是用哪个声音，顺带回答了列表本身答不了的那半个问题：这个你装了没有。选一个已经装好的，就切过去开始念；选一个没装的，就当场装——用这台机器上有的 `uv` / `pipx` / `pip`，包括那个包挑剔的 Python 版本，以及它自己不带的音色模型。每条命令跑之前都写出来，跑的时候也看得见：

```
● Bash uv tool install --python 3.12 kokoro-tts  1/1  ·  24.8s
  Installed 61 packages in 3.42s
   + kokoro-tts==0.9.4

○ kokoro 装好了，用了 25 秒。朗读已开启：kokoro · zf_xiaoxiao
```

命令要是装到了不在 PATH 上的地方（`uv` 和 `pipx` 都爱用 `~/.local/bin`），readio 会把它真正的位置记下来，而不是让你去改 shell 配置再来一遍。`openai` 是唯一的例外：那是你自己起的服务，readio 会直说装不了，而不是假装能装。

会多种语言的模型，仍然得有人告诉它现在看的是哪一种，而它的默认往往不是你要的那种：`kokoro-tts` 默认按 `en-us` 走，中文丢给它就是用英文的字母发音规则去拼——同样一句十八个字，这么念要 13.6 秒，正常念只要 4.1 秒。readio 改成每句话自己判断，而且音色跟着语种一起换，因为在 Kokoro 里这本来就是同一个决定。中英混排的一章，中途自己就换了，你什么都不用设：

```
❯ auto (active)  跟着每段文字的语种换音色
  en             固定用这一种语言念，音色 af_heart
  zh             固定用这一种语言念，音色 zf_xiaoxiao
```

`auto` 是 `/voice` 里的一行，而不只是配置文件里的一个值——一个只能开不能关的设置就是个坑：在这一行出现之前，`/voice af_heart` 是扇单向门，只有文本编辑器能把它推回去。

朗读时，正在读的那句浅高亮，正在发音的那个词或那个字深高亮；吐字速度跟着每段音频的真实时长走，不是估的。

倍速就是有声书那一套。`^r` 在 0.75×、1×、1.25×、1.5×、2× 之间循环（和网页播放器同一组档位），`/rate` 接受 0.5 到 3.0 的任意值；播放时倍速就写在状态栏引擎旁边。因为音频是**按倍速合成**的，不是播放时变速，所以改倍速会把已经预取的片段全部作废，并从你正在听的那一句重新排队——新倍速在一句之内就到，而不是等到下一段。

句子是提前合成的（`tts.prefetch`，默认 2 句），跑在单独的线程上，所以句与句之间不会留下一个正好等于引擎合成时长的空洞。

`/device` 把出声限制在指定的音频输出上。耳机断开、系统悄悄切回外放时，readio 会静音、说出它探测到的设备、并把解除办法留在屏幕上。认不出来的设备一律算不允许。

## 配置

只有一个文件 `~/.readio/config.yaml`，首次运行时带注释生成。readio 不读任何环境变量。界面默认英文，`language: zh` 换成中文。`/speed`、`/voice`、`/rate`、`/device` 这些命令会把改动写回同一个文件。

## 开发

```sh
cd apps/tui
cargo test                                     # 240 个测试
python3 scripts/pty_probe.py 96 24 "wait:0.6,type:/sample,key:enter,wait:2"
```

这个探针在真实 pty 里驱动二进制，把它画出的那一屏打印出来，包括哪些格子被高亮了。这里真正会出问题的是 raw mode、alternate screen 和退出时终端有没有还原——单元测试看不到这些。CI 在 macOS 和 Linux 上跑全套测试，两边都跑探针。

## 仓库结构

`apps/tui` 是这个阅读器；[`apps/tui/README.md`](apps/tui/README.md) 是它的实现说明——模块地图、各层守住的不变量、怎么测、怎么发（英文）。

`apps/web`、`apps/api`、`apps/extension` 是这个仓库最初的 Speechify-like 网页栈，安装说明在 [`docs/web-api-extension.md`](docs/web-api-extension.md)。

## 协议

MIT，整个仓库都是。见 [`LICENSE`](LICENSE)。
