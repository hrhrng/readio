# readio

**一个终端阅读器，交互语法照着 coding agent 做。** 你按回车，它「思考」、发起一次工具调用、然后把书里的下一段流式吐出来。

[![tui-ci](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml/badge.svg?branch=main)](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml)
[![release](https://img.shields.io/github/v/release/hrhrng/readio?filter=tui-v*&label=release&color=6f5ec7)](https://github.com/hrhrng/readio/releases)
[![license](https://img.shields.io/badge/license-MIT-6f5ec7)](apps/tui/LICENSE)
[![rust](https://img.shields.io/badge/rust-1.85%2B-6f5ec7)](https://www.rust-lang.org)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-6f5ec7)

[English](README.md)

支持 EPUB、文字型 PDF、Markdown 和纯文本。朗读交给你自己选的本地模型，正在读的那句浅高亮、读到的那个字深高亮。插图直接画在终端里。一个 3.9 MB 的二进制，不带模型、不带资源、没有运行时依赖，也不读任何环境变量。

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
| `^s` | 开关朗读 |
| `^p` `^n` · `^l` · `^c` `^d` | 输入历史 · 清屏 · 退出 |

| 命令 | 用途 |
| --- | --- |
| `/lib` `/open <n>` `/import <path>` `/forget <n>` | 管理书库 |
| `/toc` `/goto <n>` `/next` `/prev` | 章节间移动 |
| `/find <词>` | 全书检索 |
| `/auto` `/speed <n>` | 自动续读 · 吐字速度 |
| `/context` `/progress` `/plan` | 现在读到哪儿 |
| `/tts` `/voice <名字>` `/rate <0.5-3>` `/device` | 朗读与音频输出 |
| `/lang en\|zh` `/help` `/quit` | 界面语言 · 帮助 · 退出 |

## 伪装成什么

| 阅读器里的概念 | 屏幕上的说法 |
| --- | --- |
| 读到全书的百分之几 | `ctx 23.3%`，context window 占用 |
| 一段有多少字 | `735 tok`，token 数 |
| 打开这本书多久了 | `0:15`，会话计时 |
| 取下一段正文 | 一次工具调用：`● Read book.epub#ch1  L1-9  ·  0.3s` |
| 全文检索 | 你提的那个问题，命中数是真的 |

## 朗读

readio 自己不带语音模型，只按配置文件里的命令模板去调用你装好的引擎，所以换模型是改一行配置，而不是等一个新版本。

| 引擎 | 体积 · 协议 | 说明 |
| --- | --- | --- |
| `kokoro` | 82M · Apache-2.0 | 默认；多语种，长文本质量最好 |
| `piper` | ~15M · GPL-3.0 | 出声最快；文本走 stdin |
| `supertonic` | 99M · MIT | 纯 ONNX，不依赖 torch，31 种语言 |
| `openai` | — | 任何 OpenAI 兼容的 `/v1/audio/speech` 服务 |

朗读时，正在读的那句浅高亮，正在发音的那个词或那个字深高亮；吐字速度跟着每段音频的真实时长走，不是估的。

`/device` 把出声限制在指定的音频输出上。耳机断开、系统悄悄切回外放时，readio 会静音、说出它探测到的设备、并把解除办法留在屏幕上。认不出来的设备一律算不允许。

## 配置

只有一个文件 `~/.readio/config.yaml`，首次运行时带注释生成。readio 不读任何环境变量。界面默认英文，`language: zh` 换成中文。`/speed`、`/voice`、`/rate`、`/device` 这些命令会把改动写回同一个文件。

## 开发

```sh
cd apps/tui
cargo test                                     # 181 个测试
python3 scripts/pty_probe.py 96 24 "wait:0.6,type:/sample,key:enter,wait:2"
```

这个探针在真实 pty 里驱动二进制，把它画出的那一屏打印出来，包括哪些格子被高亮了。这里真正会出问题的是 raw mode、alternate screen 和退出时终端有没有还原——单元测试看不到这些。CI 在 macOS 和 Linux 上跑全套测试，两边都跑探针。

## 仓库结构

`apps/tui` 是这个阅读器；[`apps/tui/README.md`](apps/tui/README.md) 讲它的架构、伪装词表怎么映射、分发怎么做又怎么验证。

`apps/web`、`apps/api`、`apps/extension` 是这个仓库最初的 Speechify-like 网页栈，安装说明在 [`docs/web-api-extension.md`](docs/web-api-extension.md)。

## 协议

MIT，见 [`apps/tui/LICENSE`](apps/tui/LICENSE)。
