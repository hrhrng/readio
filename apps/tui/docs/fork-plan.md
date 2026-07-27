# readio: 从 grok-build 抽壳的可行性结论

来源仓库：`/Users/minimax/oos-proj/grok-build`（Apache-2.0）
分析对象：`crates/codegen/xai-grok-pager*`

## 一句话结论

不要 fork 整个 pager，要 fork 它的**渲染层 + scrollback 层**，然后自己写 app 层。
可复用约 10.5 万行，需要重写的是 17.5 万行的 `app/`。

## 分层实测

| 层 | 路径 | 行数 | 对 agent runtime 的依赖 | 判断 |
| --- | --- | --- | --- | --- |
| 渲染原语 | `xai-grok-pager-render` | 37,743 | 约 20 个调用点（config 路径、telemetry、1 个 permission 常量、图片校验） | **直接 fork**，切依赖只需 stub 5 个函数 |
| 输入框 | `xai-ratatui-textarea` | 12,722 | 无 | **直接 fork**（只依赖 ratatui / textwrap / unicode-*） |
| inline 视口 | `xai-ratatui-inline` | 2,979 | 无 | **直接 fork**，原生 scrollback 打印靠它 |
| Markdown | `xai-grok-markdown(-core)` | 1,070+ | 无（只有 pulldown-cmark） | **直接 fork** |
| scrollback | `xai-grok-pager/src/scrollback` | 51,549 | **只有 8 处泄漏** | **fork，改 8 行** |
| minimal 模式 | `xai-grok-pager-minimal` | 6,194 | 深读 `AppView` | 抄思路，不抄代码 |
| app 层 | `xai-grok-pager/src/app` | 175,436 | 90 个文件 `use xai_grok_shell` | **重写** |

`scrollback` 里的 8 处泄漏，全是浅层类型和一个 config 读取：

- `blocks/context_info.rs` → `xai_grok_shell::session::ContextInfo`
- `blocks/tool/use_tool.rs`、`search_tool.rs` → `xai_grok_workspace::permission::mcp_titleize_segment`
- `blocks/tool/read.rs` → `xai_grok_tools::…::skill_name_from_path`
- `text_selection.rs` → `xai_grok_shell::config::load_effective_config`
- `block.rs` → `ContextInfo`（两处）

替换成本几乎为零：自己定义一个 `ContextInfo` 结构体 + 两个字符串工具函数即可。

## 关键发现：模块解析靠 re-export

`scrollback` 里写的是 `crate::theme` / `crate::render` / `crate::appearance`，而 pager 的 `lib.rs:66` 只是把 render crate 原样 re-export：

```rust
pub use xai_grok_pager_render::{
    appearance, clipboard, gboom, glyphs, host, link_opener, modal_window_state,
    prompt_images, render, syntax, terminal, theme, util,
};
```

所以把 scrollback 搬进 readio 之后，只要在自己的 lib 根上做同样的 re-export，全部 `crate::…` 引用都不用改。

## 关键发现：驱动 scrollback 的接口天生适合"假 agent"

`ScrollbackState` 暴露的就是一套流式写入 API（`scrollback/state/mod.rs`）：

```rust
let id = sb.start_streaming_agent();      // 开一个流式气泡
sb.push_chunk_to_agent(id, chunk);        // 逐段吐字
sb.push_chunk_to_thinking(id, chunk);     // 思考流
sb.finish_running(id);                    // 收尾（可带耗时）
sb.push_block(RenderBlock::read(...));    // 伪造一次工具调用
sb.tick();                                // 动画帧
sb.prepare_layout(w, h);                  // 布局
ScrollbackPane::new().render_with_scratch(...); // 绘制
```

`RenderBlock` 是 13 个纯数据变体（UserPrompt / AgentMessage / ToolCall / Thinking / System / SessionEvent / BgTask / Subagent / Workflow / Btw / ContextInfo / CreditLimit / Stub），不绑任何 agent 事件枚举。也就是说：**readio 的阅读器只要把章节文本喂进 `push_chunk_to_agent`，把"翻页/检索/加载章节"包成 ToolCall block，就自动获得 Grok 的全部手感**（折叠、选择、搜索、动画、mermaid、语法高亮）。

## 建议的 crate 布局

```
readio/
  crates/
    readio-render      # fork: pager-render（切掉 config/telemetry/tools）
    readio-textarea    # fork: xai-ratatui-textarea
    readio-inline      # fork: xai-ratatui-inline
    readio-markdown    # fork: xai-grok-markdown-core
    readio-scrollback  # fork: pager/src/scrollback（改 8 处）
    readio-book        # 新写: EPUB 解析、章节索引、进度库(rusqlite)、图片抽取
    readio-app         # 新写: 事件循环、状态机、伪 tool-call 编排、短语级吐字器
    readio-bin         # 新写: 组合根
```

## 构建环境注意

原仓库 `cargo check -p xai-grok-pager-render` 会失败，因为它经 `xai-grok-tools` 拉到 `xai-grok-tools-api`，而那个 crate 的 build.rs 需要 `protoc`（仓库里的 `bin/protoc` 是 dotslash 壳，本机没装）。两条路：

- 想在原仓库跑起来：`brew install protobuf` 或 `cargo install dotslash`
- readio 里切掉 `xai-grok-tools` 依赖后，**protoc 需求随之消失**——这也是"不要整仓库 fork"的另一个理由

## 下一步

1. 建 workspace 骨架，先只搬 `readio-render` + `readio-textarea` + `readio-inline`，跑通一个空壳全屏界面。
2. 再搬 `readio-scrollback`，用假数据（thinking → tool call → 流式正文）验证手感。
3. 最后接 EPUB 层，把章节流映射成 agent 叙事。
