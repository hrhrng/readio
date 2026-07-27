# readio

**一个终端阅读器，交互语法照着 coding agent 做。** 你按回车，它「思考」、发起一次工具调用、然后把书里的文字流式吐出来。

[![tui-ci](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml/badge.svg?branch=main)](https://github.com/hrhrng/readio/actions/workflows/tui-ci.yml)
[![release](https://img.shields.io/github/v/release/hrhrng/readio?filter=tui-v*&label=release&color=6f5ec7)](https://github.com/hrhrng/readio/releases)
[![license](https://img.shields.io/badge/license-MIT-6f5ec7)](apps/tui/LICENSE)
[![rust](https://img.shields.io/badge/rust-1.85%2B-6f5ec7)](https://www.rust-lang.org)
![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-6f5ec7)
![size](https://img.shields.io/badge/binary-3.9MB-6f5ec7)

EPUB、文字型 PDF、Markdown、纯文本；本地模型朗读，读到哪儿高亮到哪儿；插图直接画在终端里。一个 3.9MB 的二进制，不带模型、不带资源、没有运行时依赖，也不读任何环境变量。

界面上看到的每一个数字都是真的——真实的段落偏移、真实的行号区间、真实的全文检索命中数。伪装的只是叙事外壳，不是数据。

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

（这一屏是 `apps/tui/scripts/pty_probe.py` 从一个真实 pty 里抓下来的，不是手写的。）

## 安装

```bash
# 装好就能用，不需要 Rust 工具链
curl -fsSL https://raw.githubusercontent.com/hrhrng/readio/main/apps/tui/scripts/install.sh | sh

# 或者自己编译（需要 Rust 1.85+）
cargo install --git https://github.com/hrhrng/readio readio
```

装的东西只有一个：`~/.local/bin/readio`，3.9MB，不带模型、不带资源、没有运行时依赖。要卸载就删掉这个文件，再删 `~/.readio`。

## 用法

```bash
readio                 # 进书库，列出导入过的书
readio book.epub       # 导入并开始读（默认 -c：复制一份到 ~/.readio/books）
readio book.pdf -l     # -l 引用：不复制，只记一条指向原路径的记录
readio book.md  -m     # -m 移动：搬进书库，原文件不再保留
```

支持 `.epub`、`.pdf`（文字型）、`.txt`、`.md`；书里的插图用半块字符直接画在终端里。不想先找书就 `/sample`，有一篇内置的短文。

**回车继续读下一段，输入文字则当成提问去全文检索。** `esc` 打断，`^t` / `^o` 折叠思考与工具调用，`^s` 开关朗读，`^c` 退出，`/help` 是完整的命令表。

## 伪装成什么

| 阅读器里的概念 | 屏幕上的说法 |
| --- | --- |
| 读到全书的百分之几 | `ctx 23.3%`，context window 占用 |
| 这一段有多少字 | `735 tok`，token 数 |
| 打开这本书多久了 | `0:15`，会话计时 |
| 取下一段正文 | 一次工具调用：`● Read book.epub#ch1  L1-9  ·  0.3s` |
| 全文检索 | 你提的问题，命中数是真的 |

## 朗读

用你自己装的本地模型（kokoro / piper / supertonic，或任何 OpenAI 兼容端点）——readio 不打包模型，只按配置里的命令模板去调用，换模型是改一行配置而不是等一个新版本。

正在读的那句浅高亮，读到的那个字深高亮，吐字速度跟着音频真实时长走。`/device` 可以设置音频输出白名单：拔了耳机、系统悄悄切回外放，它会静音并把话说清楚，而不是把书念给整间屋子。

## 配置

只有一个文件：`~/.readio/config.yaml`，**没有任何环境变量**。界面默认英文，`language: zh` 换成中文；`/speed`、`/voice`、`/rate`、`/device` 这些命令会自己写回去。

细节都在 [`apps/tui/README.md`](apps/tui/README.md)：架构、伪装词表怎么映射、分发怎么做、以及那套在真 pty 里跑的验证。

## 这个仓库里还有什么

`apps/web`、`apps/api`、`apps/extension` 是同名的 Speechify-like 网页栈，说明搬到了 [`docs/web-api-extension.md`](docs/web-api-extension.md)。
