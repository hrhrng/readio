# PRD: Readio Dashboard

## Background & Context

Readio 是一个开源的有声书/TTS 阅读平台。后端 API（FastAPI + SQLite）已完成，支持内容导入（URL/PDF/EPUB/TXT）、多 TTS 引擎（Edge/ElevenLabs/MiniMax）、异步合成队列等能力。Chrome 扩展也已可用。

**现在缺少的是 Web 端**。用户需要一个类 Apple Books 风格的 Dashboard，作为内容管理和入口页面，然后点击某本书进入播放器页面。

### Why Now

- 后端 API 已稳定，Chrome 扩展已验证核心流程
- Web 端是平台化的前提，没有 Web 端就无法面向公众开放
- 竞品（Speechify、NaturalReader）都以 Web 端为核心体验

---

## Product Overview

Dashboard 是 Readio Web 端的核心页面，采用 **左侧边栏 + 右侧主区域** 的经典布局，参考 Apple Books 桌面端设计。

**两个页面的关系：**
- **Dashboard**（本 PRD）：内容管理、浏览、搜索
- **Player**（另一份 PRD）：点击书籍后进入的有声书播放器页面

---

## Page Structure

```
┌──────────────────────────────────────────────────┐
│  Sidebar (240px fixed)  │  Main Content Area     │
│                         │                        │
│  🔍 Search              │  (根据左侧选择切换)      │
│  🏠 Home                │                        │
│                         │  - Home View           │
│  ── Library ──          │  - Search View         │
│  📚 All                 │  - Library Grid View   │
│  📖 Want to Read        │  - Collection View     │
│  ✅ Finished            │                        │
│                         │                        │
│  ── By Type ──          │                        │
│  🌐 Web                 │                        │
│  📄 PDF                 │                        │
│  📕 EPUB                │                        │
│  📝 TXT                 │                        │
│                         │                        │
│  ── Collections ──      │                        │
│  📁 Untitled Collection │                        │
│  ＋ New Collection      │                        │
│                         │                        │
│  ── User ──             │                        │
│  👤 User Avatar + Name  │                        │
└──────────────────────────────────────────────────┘
```

---

## Sidebar

固定宽度 240px，暗色背景，始终可见。

### 顶部导航
| 项目 | 行为 |
|------|------|
| Search | 点击后右侧切换为 Search View |
| Home | 点击后右侧切换为 Home View（默认landing） |

### Library（系统内置分类）

按阅读状态分类：

| 分类 | 筛选逻辑 | 图标 |
|------|----------|------|
| All | 所有 items | 书架图标 |
| Want to Read | status = `want_to_read` | 时钟/书签图标 |
| Finished | status = `finished` | 勾选图标 |

按内容类型分类：

| 分类 | 筛选逻辑 | 图标 |
|------|----------|------|
| Web | type = `web` | 地球图标 |
| PDF | type = `pdf` | 文档图标 |
| EPUB | type = `epub` | 书本图标 |
| TXT | type = `txt` | 文本图标 |

点击任意分类 → 右侧显示对应的 **Library Grid View**。

### Collections（自定义收藏夹）

- 显示用户创建的 Collection 列表
- 每个 Collection 有名称，可点击进入 Collection View
- `+ New Collection` 按钮创建新收藏夹
- 支持重命名、删除操作（右键菜单或 hover 出现操作按钮）

### 底部用户区

- 用户头像 + 名称
- 点击可展开用户菜单（设置、登出等 — 后续迭代）

---

## Main Content Area

根据左侧 Sidebar 选择，右侧显示不同的 View。

### 1. Home View（默认）

分段式布局，纵向滚动，每段独立横向滚动。参考 Apple Books Home 页面。

#### Section: Continue

- **显示条件**：status = `reading` 且 progress > 0 的 items
- **排序**：按最后阅读时间倒序
- **卡片样式**：封面缩略图 + 标题 + 作者 + 类型 + 进度百分比
- **交互**：点击 → 跳转播放器页面，从上次位置继续

#### Section: Want to Read

- **显示条件**：status = `want_to_read`
- **卡片样式**：大封面图
- **交互**：点击 → 跳转播放器页面

#### Section: Recently Added

- **显示条件**：按 created_at 倒序，最近添加的 items
- **卡片样式**：封面 + 标题 + "NEW" 徽章
- **最多显示**：横向一排，可滚动

#### Section: Reading Goals（后续迭代）

- 预留位置，MVP 可先显示占位 UI
- 后续可添加每日阅读目标、统计等

### 2. Search View

点击 Sidebar 的 Search 后显示。

- **顶部**：全宽搜索输入框，居中显示，带清除按钮
- **实时搜索**：输入后即时过滤，debounce 300ms
- **结果列表**：列表视图（非网格），每行显示：
  - 小封面缩略图（约 60x80px）
  - 标题（粗体）
  - 作者
  - 类型标签（Web / PDF / EPUB / TXT）
  - 状态标签（NEW / 进度百分比）
- **搜索范围**：标题、内容前100字
- **空状态**：未输入时不显示任何结果

### 3. Library Grid View

点击 Sidebar Library 下任意分类后显示。

- **页面标题**：显示当前分类名（如 "All"、"PDF" 等）
- **右上角**：排序/视图切换按钮
  - 排序：标题 A-Z / 最近添加 / 进度
  - 视图：网格 / 列表（MVP 先做网格）
- **网格布局**：
  - 响应式列数（根据容器宽度）
  - 每个卡片：封面大图 + 底部进度条或 "NEW" 徽章
  - Hover：显示操作菜单按钮（...）
- **操作菜单**（点击 ... 或右键）：
  - 标记为"想读" / "已读完"
  - 添加到 Collection
  - 删除
- **空状态**：该分类无内容时显示友好提示

### 4. Collection View

与 Library Grid View 布局相同，但：
- 标题显示 Collection 名称
- 支持从 Collection 中移除 item（操作菜单增加"从收藏夹移除"）

---

## Data Model Changes

### LibraryItem 新增字段

```
status: 'new' | 'want_to_read' | 'reading' | 'finished'
  - 默认值: 'new'
  - 'new': 新导入，未阅读
  - 'want_to_read': 用户标记想读
  - 'reading': 用户开始阅读后自动切换（progress > 0）
  - 'finished': 用户手动标记或 progress = 100

author: string | null
  - 从导入内容中提取，或用户手动填写

cover_url: string | null
  - 封面图 URL，从 EPUB 提取或用户上传
  - 无封面时前端显示 placeholder

last_read_at: datetime | null
  - 最后阅读时间，用于 Continue 排序
```

### 新增 Collection 模型

```
Collection:
  - id: string (UUID)
  - name: string
  - created_at: datetime
  - updated_at: datetime

CollectionItem (多对多关系):
  - collection_id: string
  - item_id: string
  - added_at: datetime
```

### 新增 API Endpoints

```
# Collection CRUD
GET    /api/library/collections
POST   /api/library/collections
PUT    /api/library/collections/{id}
DELETE /api/library/collections/{id}

# Collection Items
GET    /api/library/collections/{id}/items
POST   /api/library/collections/{id}/items
DELETE /api/library/collections/{id}/items/{item_id}

# Item status
PATCH  /api/library/items/{id}/status
  body: { status: 'want_to_read' | 'reading' | 'finished' }
```

---

## UI Specifications

### Theme

支持亮色和暗色双主题，默认跟随系统偏好，用户可手动切换。

**亮色模式**（参考 Apple Books Light）：
- 背景：#FFFFFF（主区域），#F5F5F7（Sidebar）
- 文字：#1D1D1F（主标题），#86868B（次要文字）
- 强调色：#007AFF
- 卡片：白色背景 + `shadow-sm`
- 分隔线：#E5E5EA

**暗色模式**（参考 Apple Books Dark）：
- 背景：#1C1C1E（主区域），#000000（Sidebar）
- 文字：#FFFFFF（主标题），#8E8E93（次要文字）
- 强调色：#0A84FF
- 卡片背景：#2C2C2E，无阴影
- 分隔线：#38383A

### Typography

- 页面标题：28-34px，bold，serif（参考 Apple Books 的 "All"、"Home" 标题风格）
- Section 标题：20-22px，bold
- 卡片标题：14px，medium
- 副文字：12px，regular，灰色

### 封面卡片

- 网格模式下封面宽度约 150-180px，高度按比例
- 圆角 8px
- 无封面时显示渐变背景 + 标题文字的 placeholder
- 进度显示在封面下方，格式："14%" 或 "NEW" 徽章（蓝色）
- 底部有 "..." 操作菜单按钮

### 动效

- Sidebar 选中切换：主区域内容淡入（150ms ease）
- 卡片 hover：轻微放大（scale 1.02）+ 阴影加深
- 搜索输入：focus 时搜索框展开动画
- 列表加载：骨架屏占位

---

## Tech Stack

- **Framework**: Next.js (App Router)
- **Styling**: Tailwind CSS
- **State**: React Context 或 Zustand（轻量状态管理）
- **API Client**: fetch + SWR 或 React Query（缓存 & 自动刷新）
- **Icons**: Lucide React

---

## Success Criteria

1. 用户能通过 Dashboard 浏览所有已导入的内容
2. 搜索能在 300ms 内返回结果并展示
3. 用户能通过状态筛选（想读/在读/已完成）快速定位内容
4. 用户能创建 Collection 并管理书籍分组
5. 点击任意书籍能跳转到播放器页面（本 PRD 不涉及播放器实现）
6. 暗色主题下视觉体验接近 Apple Books 质感

---

## Out of Scope (MVP)

- 用户认证 / 多用户体系
- Reading Goals 统计功能
- 拖拽排序
- 批量操作（批量删除、批量移动）
- 国际化 / 多语言
- 响应式移动端适配（先做桌面端）
- 内容导入功能的 UI（MVP 阶段通过 API 或扩展导入）

---

## Decisions

### 封面图策略

- **EPUB**：从 EPUB 元数据中提取封面图，存储为 cover_url
- **PDF**：提取第一页渲染为缩略图作为封面
- **Web / TXT**：前端生成 placeholder 封面 —— 基于内容类型选择渐变色背景 + 居中显示标题文字 + 左下角类型标签
  - Web 渐变：深蓝 → 靛蓝
  - TXT 渐变：深灰 → 暖灰
- 所有类型都支持用户后续手动上传自定义封面（通过操作菜单）

### 播放器跳转

路由跳转方式：点击书籍 → 导航到 `/player/{item_id}`。Dashboard 和 Player 是两个独立路由页面。

### Home 页内容推荐

不做内容推荐。Home 页仅展示用户自己的内容（Continue / Want to Read / Recently Added）。
