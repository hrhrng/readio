# PRD: Readio Reader + Audio Player

## Background & Context

Readio 已完成后端 API（TTS 合成 + 异步任务队列 + 内容导入）、Chrome 扩展、以及 Web Dashboard。用户可以在 Dashboard 浏览和管理导入的内容，但点击任何一本书后只看到一个 "Player Coming Soon" 占位页。

**Reader 是 Readio 的核心体验** —— 没有它，整个平台只是一个书架。用户需要：
1. 阅读已导入的 web/txt/epub/pdf 内容
2. 点击任意一句话，从该句开始 TTS 播放
3. 实时看到「正在读哪句、读到哪个词」的视觉反馈

### Why Now

- Dashboard 已上线，用户流程断在 "点击书 → 空白页"
- 后端 TTS 基础设施已完备：同步合成、SSE 流式、异步任务队列（带 priority + 缓存 + session 取消）
- 竞品（Speechify、NaturalReader）的核心差异化就在 Reader + 播放体验

---

## Product Overview

Reader 页面 (`/player/{item_id}`) 替换当前占位页，提供：

1. **多格式内容渲染**：web、txt、epub、pdf 的纯文本阅读视图
2. **TTS 音频播放**：点击任意句子开始播放，句子级 + 词级双层高亮
3. **预取窗口**：自动预合成后续句子的音频，保证连续播放无停顿
4. **底部播放栏**：播放/暂停、前后句跳转、进度条、速度调节、预估剩余时间

```
┌─────────────────────────────────────────────────────┐
│ ← Back          Title of the Book          ⚙ ...   │  ← Top Bar
├──────────┬──────────────────────────────────────────┤
│          │                                          │
│  TOC     │   正文内容区域                             │  ← Reading Area
│  Panel   │                                          │
│ (toggle) │   普通句子普通句子普通句子。                  │
│          │   ██当前句子高亮██当前词更亮██。              │
│          │   普通句子普通句子。                         │
│          │                                          │
├──────────┴──────────────────────────────────────────┤
│  ◁ prev │ ▶ Play/Pause │ ▷ next │ ██░░ 23% │ 1.0x │  ← Player Bar
│                              Est. 12:30 remaining   │
└─────────────────────────────────────────────────────┘
```

---

## Content Rendering

### 统一文本模型

不论原始格式是什么，Reader 都工作在 **纯文本** 之上。后端 `content` 字段已经保存了提取后的纯文本。

在前端做两步处理：
1. **分句**（Sentence Splitting）：按标点（`.!?。！？`）+ 换行分割为句子数组
2. **分词**（Word Splitting）：每个句子再按空格 / 字符边界分割为词数组

```typescript
interface Sentence {
  index: number;        // 句子在全文的序号
  text: string;         // 完整句子文本
  words: Word[];        // 词数组
}

interface Word {
  index: number;        // 词在句子内的序号
  text: string;         // 词文本（含尾随空格/标点）
  startOffset: number;  // 在句子内的字符起始位
  endOffset: number;    // 在句子内的字符结束位
}
```

### 按格式的处理策略

| 格式 | 内容来源 | 渲染方式 |
|------|---------|---------|
| **web** | 后端 `content` 已提取纯文本 | 直接渲染，段落由 `\n\n` 分隔 |
| **txt** | 后端 `content` 即原文 | 直接渲染，保留换行 |
| **epub** | 后端已将 EPUB 解析为纯文本 | 直接渲染 |
| **pdf** | 后端 PyMuPDF 提取的文本 | 直接渲染；对于格式复杂的 PDF（表格、多栏），接受文本可能不完美 |

**PDF 特殊处理**：
- 后端已用 PyMuPDF 提取文本，结果存于 `content`
- 前端不尝试还原 PDF 原始排版，统一走纯文本渲染
- 在 Reader 顶部显示提示："This PDF was converted to text. Some formatting may be lost."
- 未来可考虑双视图模式（PDF 原始 + 文本），但 MVP 不做

### 段落与排版

- 用 `\n\n` 分段，每段作为一个 `<p>` 渲染
- 段落内的句子各包裹在 `<span>` 中，用于高亮定位
- 字体使用 `font-serif`（Playfair Display body text 不适合长文，使用系统 serif 或 Georgia）
- 正文字号 `18px`，行高 `1.8`，最大宽度 `720px` 居中
- 亮色背景 `--surface`，暗色背景 `--surface`（跟随主题）

---

## Audio Playback System

### 核心流程

```
用户点击句子 N
  → 立即播放句子 N（如有缓存则直接播放，否则发起 user 优先级请求）
  → 预取窗口启动：发起句子 N+1 ~ N+K 的 prefetch 请求
  → 句子 N 播放完毕 → 自动跳到句子 N+1
  → 预取窗口滑动：取消超出窗口的旧请求，发起新的 prefetch
  → 用户手动跳转 → 取消当前 session 的旧 prefetch，从新位置重启
```

### 利用后端 Job Queue

后端 `POST /api/tts/jobs` 已支持：
- `priority: 'user' | 'prefetch'` — user 优先级的 job 排在 prefetch 前面
- `session_id` — 可按 session 批量取消
- `item_id` + `chapter_id` — 用于标识文本段
- `cache_hit` — 相同文本 + voice + speed 会命中缓存
- `POST /api/tts/sessions/{session_id}/cancel` — 取消整个 session 的 prefetch

**前端使用模式**：
- 每次进入 Reader 页面生成一个 `session_id`
- 当前正在播放的句子 → `priority: 'user'`
- 预取窗口内的句子 → `priority: 'prefetch'`
- `chapter_id` 使用 `sentence-{index}` 格式
- 用户跳转时调用 `cancel_session`（保留当前句子），然后提交新窗口

### 本地音频缓存

```typescript
interface AudioCache {
  // key: sentence index, value: decoded AudioBuffer or base64 string
  cache: Map<number, AudioBuffer>;
  maxSize: number;  // 默认 50 句

  get(sentenceIndex: number): AudioBuffer | undefined;
  set(sentenceIndex: number, buffer: AudioBuffer): void;
  evict(): void;  // LRU 淘汰
}
```

- 缓存大小默认 50 句（约 ~25MB 内存，按每句平均 500KB 估算）
- 淘汰策略：LRU，优先保留播放方向前方的句子
- 用户跳转到远处时，清空旧缓存区域

### 预取窗口

- **窗口大小 K = 5**（当前句子后方 5 句）
- 每当播放指针移动，计算需要预取的新句子，增量发起请求
- 已在缓存中的句子不重复请求
- 用户大幅跳转时（比如点击 TOC 跳转）：
  1. 调用 `cancel_session`（保留新位置的句子）
  2. 清空无关缓存
  3. 从新位置重新启动预取窗口

---

## Sentence & Word Highlighting

### 双层高亮

播放进行时，视觉上呈现两层高亮：

1. **句子高亮**（Sentence-level）：当前正在播放的整句话背景高亮
   - 亮色模式：`bg-accent/10` 浅蓝底色
   - 暗色模式：`bg-accent/15` 略亮蓝底色

2. **词高亮**（Word-level）：当前正在读的词加粗 + 颜色强调
   - 亮色模式：`text-accent font-semibold`
   - 暗色模式：`text-accent font-semibold`

### 词级定位方案

TTS 引擎通常不返回词级时间戳（Edge TTS 除外，但 MiniMax 和 ElevenLabs 不提供）。采用 **估算方案**：

1. 获取句子音频的总时长 `duration_ms`（后端 `SynthesizeResponse.duration_ms` 或从 AudioBuffer 获取）
2. 按词的字符数比例分配时间：
   ```
   word_duration = sentence_duration * (word.length / sentence.total_chars)
   ```
3. 播放时用 `requestAnimationFrame` + `AudioContext.currentTime` 实时追踪进度
4. 根据当前播放时间点计算应高亮第几个词

这是估算，不会 100% 精确，但视觉效果已经足够好。

### 自动滚动

当高亮句子即将滚出可视区域时，平滑滚动 `scrollIntoView({ behavior: 'smooth', block: 'center' })`。

---

## Player Bar（底部播放栏）

固定在页面底部，高度约 72px。

### 布局

```
┌─────────────────────────────────────────────────────────────┐
│  ← prev  │  ▶ / ❚❚  │  next →  │  ████░░░░ 23%  │  1.0x  │
│                           Est. ~12:30 remaining             │
└─────────────────────────────────────────────────────────────┘
```

### 控件

| 控件 | 功能 |
|------|------|
| **Play / Pause** | 切换播放状态。首次点击从当前句子开始；暂停后恢复从当前词继续 |
| **Previous** | 跳到上一句开头。如当前句子已播放 >2 秒则回到本句开头 |
| **Next** | 跳到下一句开头 |
| **Progress Bar** | 以「已播放句子数 / 总句子数」百分比显示。可拖拽跳转 |
| **Speed** | 播放速度按钮，点击循环切换：0.75x → 1.0x → 1.25x → 1.5x → 2.0x |
| **Estimated Time** | 基于已播放音频时长和剩余句子数估算。格式："Est. ~12:30 remaining" |

### 进度计算

TTS 的实际音频时长不可提前确定（取决于 provider、text complexity、speed），所以：

- **进度百分比** = `当前句子 index / 总句子数 * 100`
- **预估剩余时间**：取最近 10 句的平均音频时长，乘以剩余句子数
  ```
  avg_duration = sum(recent_10_durations) / 10
  remaining = avg_duration * (total_sentences - current_index)
  ```
- 显示格式：`Est. ~MM:SS remaining`（不足 1 分钟显示 `Est. ~0:SS`）
- 没有足够数据时显示 `Estimating...`

---

## Top Bar

固定在页面顶部。

| 元素 | 说明 |
|------|------|
| **← Back** | 返回 Dashboard（`router.back()` 或 `/`） |
| **Title** | 居中显示书名，超长截断 |
| **TOC Toggle** | 仅 epub 类型或段落 >10 时显示。点击展开/收起左侧 TOC 面板 |
| **Settings** | 字体大小调节（小/中/大），后续可扩展 |

---

## Table of Contents Panel（可选）

左侧可收起面板，宽度 240px（与 Dashboard Sidebar 一致）。

- **txt / web**：按 `\n\n` 分段，以段落首行（前 30 字符）作为目录项
- **epub**：如后端提供章节信息，按章节展示（MVP 先不做，因为后端未拆分章节）
- **pdf**：按段落展示
- 点击目录项 → 阅读区域滚动到对应段落，同时重置播放位置

MVP 阶段 TOC 面板为可选功能，优先级低于核心 Reader + Audio。

---

## State Management

### ReaderContext

```typescript
interface ReaderState {
  // Content
  item: LibraryItem | null;
  sentences: Sentence[];

  // Playback
  isPlaying: boolean;
  currentSentenceIndex: number;
  currentWordIndex: number;
  playbackSpeed: number;          // 0.75 | 1.0 | 1.25 | 1.5 | 2.0

  // Audio
  sessionId: string;
  audioCache: Map<number, AudioBuffer>;
  pendingJobs: Map<number, string>;  // sentence index → job_id
  recentDurations: number[];         // 最近 N 句的音频时长，用于估算

  // UI
  showToc: boolean;
  fontSize: 'small' | 'medium' | 'large';
}
```

使用 React Context + `useReducer` 管理，避免引入额外状态库。

---

## API Integration

### 使用的 Endpoints

| 场景 | Endpoint | 说明 |
|------|---------|------|
| 加载内容 | `GET /api/library/items/{id}` | 获取 item 的 title + content |
| 当前句子合成 | `POST /api/tts/jobs` (priority=user) | 用户触发，高优先级 |
| 预取后续句子 | `POST /api/tts/jobs` (priority=prefetch) | 后台预取，低优先级 |
| 轮询结果 | `GET /api/tts/jobs/{job_id}?include_audio=true` | 拿到 base64 音频数据 |
| 跳转时取消 | `POST /api/tts/sessions/{session_id}/cancel` | 取消旧的 prefetch 请求 |

### 轮询策略

Job 提交后轮询 `GET /api/tts/jobs/{job_id}?include_audio=true`：
- 初始间隔 200ms
- 每次轮询间隔翻倍，最大 2000ms
- 命中缓存的 job 会立即返回 `status: completed`
- 超时 30 秒仍未完成则标记为失败，跳过该句

---

## Error Handling

| 场景 | 处理 |
|------|------|
| 某句 TTS 失败 | 跳过该句，自动播放下一句。在 Player Bar 短暂显示 "Skipped: synthesis failed" |
| 网络断开 | 暂停播放，显示 "Network error. Retrying..." 并自动重试 3 次 |
| 后端不可达 | 显示 toast "Cannot connect to server"，播放暂停 |
| Content 为空 | 显示空状态 "This item has no readable content" |
| 所有 TTS provider 失败 | 显示错误提示 "TTS synthesis unavailable. Please check provider settings." |

---

## File Structure

```
apps/web/
├── app/player/[id]/
│   └── page.tsx                    # Reader 页面入口（替换现有占位）
├── components/reader/
│   ├── reader-provider.tsx         # ReaderContext provider + reducer
│   ├── reader-content.tsx          # 正文渲染区（句子 + 词 span）
│   ├── reader-topbar.tsx           # 顶部导航栏
│   ├── reader-player-bar.tsx       # 底部播放控件栏
│   ├── reader-toc.tsx              # 左侧目录面板（可选）
│   └── reader-highlight.tsx        # 句子/词高亮逻辑组件
└── lib/
    ├── sentence-parser.ts          # 分句 + 分词工具函数
    ├── audio-cache.ts              # 本地 LRU 音频缓存
    └── tts-prefetch.ts             # 预取窗口管理 + job 轮询
```

### 后端无需改动

现有 API 已完全满足需求：
- `GET /api/library/items/{id}` → content 文本
- `POST /api/tts/jobs` → 异步合成（支持 priority + session）
- `GET /api/tts/jobs/{job_id}?include_audio=true` → 获取结果
- `POST /api/tts/sessions/{session_id}/cancel` → 取消预取

---

## UI Theme Integration

Reader 页面复用 Dashboard 的 CSS 变量系统，亮暗切换自动生效。

| 元素 | 亮色 | 暗色 |
|------|------|------|
| 阅读背景 | `var(--surface)` #FFFFFF | `var(--surface)` #1C1C1E |
| 正文文字 | `var(--text-primary)` #1D1D1F | `var(--text-primary)` #FFFFFF |
| 句子高亮背景 | `accent/10` 浅蓝 | `accent/15` 深蓝 |
| 当前词 | `var(--accent)` 蓝色加粗 | `var(--accent)` 蓝色加粗 |
| Player Bar 背景 | `var(--surface-card)` + border-top | `var(--surface-card)` + border-top |
| Top Bar 背景 | `var(--surface)` + border-bottom | `var(--surface)` + border-bottom |
| TOC 面板背景 | `var(--surface-sidebar)` | `var(--surface-sidebar)` |

---

## Success Criteria

1. 用户可从 Dashboard 点击任意书 → 进入 Reader 页面看到正文内容
2. 点击任意句子 → 1 秒内开始播放（首句，后续从缓存播放应 <200ms）
3. 播放时当前句子高亮、当前词有视觉追踪
4. 连续播放时句子之间无明显停顿（预取窗口生效）
5. 播放控件（暂停/播放/前后句/速度/进度）均可正常操作
6. web、txt、epub、pdf 四种格式均可正常阅读
7. 亮暗模式下阅读体验一致

---

## Out of Scope (MVP)

- EPUB 章节目录解析（需后端支持 chapter 拆分）
- PDF 原始排版视图（双视图模式）
- 词级精确时间戳（需 TTS 引擎支持 word-level timing）
- 书签 / 笔记 / 高亮批注
- 键盘快捷键（Space 暂停等 — 可快速后续添加）
- 离线播放 / Service Worker 缓存
- 进度同步到后端（更新 `progress` 字段 — 后续迭代）
- 移动端适配

---

## Key Decisions

### 为什么用 Job Queue 而不是直接 synthesize？

`POST /api/tts/synthesize` 是同步请求，每次只能请求一句，且无法管理并发。Job Queue 提供：
- 优先级：用户当前句子优先于预取
- 并发控制：后端限制并行数（默认 2），避免打爆 TTS provider
- 缓存命中：相同文本自动复用结果
- Session 取消：用户跳转时批量取消旧请求

### 为什么词级高亮用估算？

Edge TTS 支持 word-level timing（通过 SSML），但 MiniMax 和 ElevenLabs 不提供。为保持 provider 无关性，统一用时间比例估算。视觉上 80%+ 的准确率已足够，不值得为此锁定 provider。

### PDF 为什么不做原始排版？

PDF 的排版复杂度极高（表格、多栏、公式、嵌入图片）。前端渲染 PDF 原始格式需要 pdf.js 等重型库，且与 TTS 的句子高亮逻辑冲突。MVP 统一走纯文本路线，降低复杂度。未来可做 "双视图" 切换。

---

## Library Management（Import & Delete）

### Import

用户可通过 Web App 直接导入内容，无需依赖浏览器扩展。

**入口**：Sidebar "Import" 按钮，打开 Import Dialog。

**Import Dialog**：
- 两个 Tab：**File** 和 **URL**
- File Tab：拖放区 + 文件选择器，支持 TXT、PDF、EPUB、DOCX
- URL Tab：URL 输入框 + Import 按钮，抓取网页内容
- Loading 状态：显示 spinner，禁用交互
- 错误状态：显示后端返回的错误信息
- 成功后自动关闭对话框，SWR revalidate 刷新列表

**API**：
| 操作 | Endpoint | 说明 |
|------|---------|------|
| 文件上传 | `POST /api/library/import/file` | multipart form data |
| URL 导入 | `POST /api/library/import/url` | JSON body: `{ url, category, folder_id }` |

**文件处理流程**：
1. 前端 FormData 包含 `file` + `folder_id` + `category`
2. 后端解析文件（txt 直读、pdf PyMuPDF 提取、epub 结构化解析、docx XML 提取）
3. EPUB/PDF 原始文件同时保存到 `data/files/{item_id}/` 用于前端渲染
4. 返回 `LibraryItem`

### Delete

**入口**：BookCard hover 时出现 "..." 菜单按钮。

**交互流程**：
1. Hover BookCard → 出现半透明圆形菜单按钮（grid 模式左上角，list 模式右侧）
2. 点击菜单按钮 → 弹出下拉菜单（目前仅 "Delete" 选项）
3. 点击 "Delete" → 弹出确认对话框："Delete item? ... will be permanently removed"
4. 确认删除 → 调用 `DELETE /api/library/items/{id}` → SWR revalidate 刷新
5. 取消 → 关闭对话框

**后端删除流程**：
1. 查询 item 获取 `file_path`
2. 删除数据库记录
3. 如有关联文件（EPUB/PDF 原始文件），删除磁盘文件 + 空目录清理

**UI 规范**：
- 确认对话框使用红色 "Delete" 按钮（`destructive` 样式）
- 菜单按钮 grid 模式：`w-7 h-7 bg-black/50 backdrop-blur-sm rounded-full`
- 菜单按钮 list 模式：`w-8 h-8` 图标按钮，hover 时显示
- 下拉菜单：`bg-surface-card border border-border rounded-xl shadow-lg`
- 点击菜单外部区域关闭菜单
