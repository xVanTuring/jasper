# Joplin 数据格式规范（v3.6.15）

> 本文是 **jasper**（只读客户端）的实现依据，逆向自 Joplin v3.6.15 源码
> （`joplin/packages/lib` 与 `joplin/packages/renderer`）。所有结论均附 `file:line` 引用。
> 范围限定为**只读 + 明文（不支持 E2EE）+ 本地文件夹 / WebDAV** 两种数据源。

---

## 1. 总览：同步目标是一个"扁平文件仓库"

Joplin 的同步目标（sync target）不是数据库，而是一堆**平铺在根目录**的文件：

- 每个条目（笔记 / 笔记本 / 标签 / 资源元数据 / 笔记-标签关联…）是一个独立的 `<id>.md` 文本文件。
- 资源（附件）的二进制单独放在 `.resource/` 子目录。
- 还有少量同步管理文件（`info.json`、`locks/`、`temp/`），**只读客户端基本可全部忽略**。

只读客户端要做的三件事：**枚举 `.md` 文件 → 解析条目格式 → 建树/渲染**。完全不需要复刻同步引擎。

---

## 2. 同步目标目录布局

```
<sync-root>/
├── <32位hex>.md          每个条目一个文件（笔记/笔记本/标签/资源元数据/note_tag…），平铺，无子目录
├── .resource/
│   └── <32位hex>          资源二进制，文件名=资源ID，无扩展名
├── info.json             同步元数据（版本、E2EE 状态、主密钥…）
├── locks/                同步锁（多端并发控制）—— 只读忽略
├── temp/                 临时文件（时钟偏移检测）—— 只读忽略
└── .sync/                旧格式兼容目录（version.txt 等）—— 只读忽略
```

| 路径 | 作用 | 只读客户端 | 来源 |
|---|---|---|---|
| `<id>.md` | 条目文件，平铺在根目录 | **必须读** | `models/BaseItem.ts:167-183` |
| `.resource/<id>` | 资源二进制（无扩展名） | **必须读** | `services/synchronizer/utils/resourceRemotePath.ts:3-5`, `utils/types.ts:5` |
| `info.json` | 版本/加密元数据 | 建议读 `version`，判加密 | `services/synchronizer/syncInfoUtils.ts:108-138, 278-362` |
| `locks/` | `{type}_{clientType}_{clientId}.json` | **忽略** | `services/synchronizer/LockHandler.ts:185-198` |
| `temp/` | `timeCheck*.txt` 等 | **忽略** | `file-api.ts:208-230` |
| `.sync/` | 旧版本兼容 | **忽略** | `services/synchronizer/migrations/2.ts:6-7` |

**条目文件名规则**（`models/BaseItem.ts:167-183`）：
- `systemPath()` → `<id>.md`，`id` 是 32 位十六进制。
- `isSystemPath()` 校验：去掉路径后，文件名必须是 `^[0-9a-f]{32}\.md$`。
- 枚举时**只认 32hex + `.md`** 的文件，其余（info.json、目录等）跳过。

---

## 3. 条目文件格式（核心）

### 3.1 文件结构

一个条目文件由**最多三段**组成，段与段之间用**一个空行**（`\n\n`）分隔：

```
<标题>                ← 第 1 段：标题（所有类型都有）
                      ← 空行
<正文 markdown>        ← 第 2 段：正文（仅笔记 type_=1 有；其它类型无此段）
                      ← 空行
key: value            ← 第 3 段：元数据键值对（每行一个，含 type_）
key: value
...
type_: 1
```

序列化逻辑：`models/BaseItem.ts:453-497`（`serialize`）——
`temp = [title, body, props.join('\n')].filter(存在).join('\n\n')`。

**笔记**（有正文）示例：

```
我的购物清单

- [ ] 牛奶
- [x] 鸡蛋

id: a1b2c3d4e5f60718293a4b5c6d7e8f90
parent_id: 0f1e2d3c4b5a69788796a5b4c3d2e1f0
created_time: 2024-03-01T08:30:00.000Z
updated_time: 2024-03-02T10:15:42.123Z
is_todo: 0
markup_language: 1
...
type_: 1
```

**笔记本**（无正文，标题后直接接元数据）示例：

```
工作

id: 0f1e2d3c4b5a69788796a5b4c3d2e1f0
created_time: 2024-01-10T00:00:00.000Z
updated_time: 2024-01-10T00:00:00.000Z
parent_id:
type_: 2
```

### 3.2 解析算法（逆向自 `unserialize`，`models/BaseItem.ts:589-632`）

**关键点：从文件末尾往上扫**，把结尾处连续的非空行当作元数据，遇到第一个空行就停。

```text
1. lines = content.split('\n')
2. 从 i = 最后一行 往前遍历：
     line = lines[i].trim()
     若 line == ""  →  body 段 = lines[0..i]，停止（这个空行是 正文/标题 与 元数据 的分隔）
     否则           →  这是元数据行：p = line.indexOf(':')
                       key = line[0..p].trim()，value = line[p+1..].trim()
                       存入 output[key]
                       （若该行没有 ':' → 抛错"Invalid property format"）
3. 必须存在 type_ ；output.type_ = Number(type_)
4. 若 body 段非空：
     title = body[0]            ← 第 1 行是标题
     （body[1] 是标题与正文间的空行）
5. 若 type_ == 1(NOTE)：正文 = body[2..].join('\n')
   其它类型：忽略正文，只取标题
```

**实现要点 / 坑：**
- 元数据块永远在文件**最末尾**，且是连续非空行。正文里即使出现 `xxx: yyy` 这样的行也不会被误判为元数据，因为它在分隔空行**之上**。
- 元数据行会被 `trim()`；正文行**不** trim（保留原样缩进）。
- 标题（第 1 行）按原样取，**不做反转义**（见 3.3）。

### 3.3 字段值的转义 / 时间戳（`serialize_format` / `unserialize_format`，`models/BaseItem.ts:401-450`）

读取时需对**元数据字段值**做反向处理（`body` 与 `title` 除外）：

| 处理 | 规则 |
|---|---|
| **换行反转义** | 文件里 `\n`→真实换行，`\r`→回车；`\\n`→字面 `\n`，`\\r`→字面 `\r`。（多数字段是单行，通常无需处理；`source_url`、`title`(笔记本/标签名) 等理论上可能含转义） |
| **时间戳** | 文件里是 ISO `YYYY-MM-DDTHH:mm:ss.SSSZ`（UTC）。解析为 Unix 毫秒。涉及字段：`created_time` `updated_time` `user_created_time` `user_updated_time`。空值视为 0。 |
| **经纬度** | `latitude`/`longitude` 8 位小数，`altitude` 4 位小数（仅展示用，可忽略精度） |
| `body` | **不转义**，原样 |
| `title` | **不转义**，原样（它在正文段第一行，不是元数据） |

> 反转义顺序（来自源码，Rust 实现照抄即可）：
> `\\n→\n` 再 `\\r→\r` 再 `\\\n→\\n` 再 `\\\r→\\r`。

---

## 4. type_ 枚举（`BaseModel.ts:12-29`）

| 值 | 类型 | 只读客户端关心？ |
|---|---|---|
| 1 | Note 笔记 | ✅ 核心 |
| 2 | Folder 笔记本 | ✅ 核心 |
| 3 | Setting | ❌（不出现在同步文件里） |
| 4 | Resource 资源（元数据） | ✅ 核心 |
| 5 | Tag 标签 | ✅（标签视图） |
| 6 | NoteTag 笔记-标签关联 | ✅（建立笔记↔标签关系） |
| 9 | MasterKey 主密钥 | ⏭ 明文场景忽略 |
| 13 | Revision 历史版本 | ⏭ 忽略（含 `body_diff`/`title_diff`，体量大） |
| 其它 (7,8,10,11,12,14,15,16) | 本地用，不同步 | ❌ |

> 同步会出现的类型见 `models/BaseItem.ts:68-76`。只读 MVP 只需处理 **1/2/4/5/6**。

---

## 5. 各类型字段（`services/database/types.ts`）

下面只列**只读客户端实际要用**的字段（每类还有 encryption_* / share_id / is_shared / user_data / user_*_time 等，明文只读多数可忽略）。

### 5.1 Note (type_=1) — `types.ts:223-255`
| 字段 | 含义 | 用途 |
|---|---|---|
| `id` | 32hex 主键 | 必需 |
| `title` | 标题（在正文段第 1 行） | 列表/标题显示 |
| `body` | 正文（在正文段，标题之后） | 渲染 |
| `parent_id` | 所属笔记本 id | 建树 |
| `created_time` / `updated_time` | 创建/修改时间 | 排序/显示/增量 |
| `markup_language` | 1=Markdown，2=HTML | **决定渲染方式** |
| `is_todo` / `todo_completed` / `todo_due` | 待办标记 | 待办显示（可选） |
| `is_conflict` / `conflict_original_id` | 冲突笔记 | 可标记或隐藏 |
| `source_url` | 来源 URL | 可选显示 |
| `latitude`/`longitude`/`altitude` | 地理位置 | 可忽略 |
| `order` | 自定义排序 | 排序（可选） |

### 5.2 Folder (type_=2) — `types.ts:105-122`
`id`、`title`（笔记本名）、`parent_id`（父笔记本，空=根）、`created_time`/`updated_time`、`icon`（JSON，FolderIcon，可选显示 emoji）。

### 5.3 Resource (type_=4) — `types.ts:279-304`
| 字段 | 含义 |
|---|---|
| `id` | 资源 id（= `.resource/<id>` 文件名） |
| `title` | 原始文件名/标题 |
| `mime` | MIME 类型 → **HTTP 响应的 Content-Type** |
| `file_extension` | 扩展名 |
| `size` | 字节数 |
| `updated_time` | 修改时间（用于图片 URL 的 `?t=` 缓存戳） |

> 展示用文件名 `resourceFilename()`（`models/utils/resourceUtils.ts:8`）：
> `<id中非字母数字替换为_> + ('.' + ext)`，ext 取 `file_extension`，缺失则由 `mime` 推导。
> **只读服务端只需把 `.resource/<id>` 的二进制配上 `mime` 头返回即可**，文件名非必需。

### 5.4 Tag (type_=5) — `types.ts:354-367`
`id`、`title`（标签名）、`parent_id`（嵌套标签，可选）、时间戳。

### 5.5 NoteTag (type_=6) — `types.ts:210-221`
关联表：`id`、`note_id`、`tag_id`。用来把标签挂到笔记上。

---

## 6. markup_language：Markdown vs HTML（`renderer/types.ts:3-7`）

- `1 = Markdown`（绝大多数笔记，默认）→ 走 markdown-it 渲染。
- `2 = HTML`（从网页剪藏等）→ 正文本身就是 HTML，**直接净化后展示**，不走 markdown。
- `3 = Any`（仅内部用）。

> 只读客户端务必判断此字段，HTML 笔记不能当 markdown 渲染。

---

## 7. Markdown 渲染要点（前端复刻，`packages/renderer`）

### 7.1 资源链接 `:/<id>`（最关键，`renderer/urlUtils.js:3,13-30`）
- 正则：`/^(joplin:\/\/|:\/)([0-9a-zA-Z]{32})(|#[^\s]*)(|\s".*?")$/`
- `![alt](:/<id>)` → 图片：`<img src="<资源URL>?t=<updated_time>">`，资源 URL 在我们这里就是 `GET /api/resources/<id>`。
- `[文字](:/<id>)` → 内部链接：指向另一条**笔记**则在前端拦截做应用内跳转；`#hash` 用于笔记内锚点。
- 渲染规则源码：图片 `MdToHtml/rules/image.ts`，链接 `MdToHtml/linkReplacement.ts`、`rules/link_open.ts`。

### 7.2 复刻优先级（按只读阅读体验排序）
**P0 必做**：markdown-it 核心、资源链接 `:/id` 解析、checkbox（`- [ ]`/`- [x]`，正则 `^\[([xX ])\] (.*)$`，`rules/checkbox.ts:130`）、代码高亮（highlight.js）、表格。
**P1 强烈建议**：KaTeX 数学公式（`$...$` / `$$...$$`，`rules/katex.ts`）、Mermaid 图表、脚注 footnote、`mark`(`==x==`)、`sub`/`sup`、`emoji`、`insert`(`++x++`)、frontmatter、HTML 净化（DOMPurify）。
**P2 可后置**：deflist、abbr、toc、ABC 乐谱、Fountain 剧本、YouTube 嵌入、媒体播放器（audio/video/pdf）。

### 7.3 Joplin 启用的 markdown-it 插件清单（`renderer/MdToHtml.ts:68-80, 526-530`）
内置：`html:true`、`linkify`、`typographer`、`breaks`（受 softbreaks 设置控制）。
第三方：`mark` `footnote` `sub` `sup` `deflist` `abbr` `emoji` `ins(insert)` `multimd-table` `toc` `expand-tabs`。
自定义规则：frontmatter、fence(代码块)、sanitize_html、image、checkbox、katex、link_open/close、html_image、code_inline、fountain、abc、mermaid、externalEmbed 等。
- 代码高亮：**highlight.js v11**（`renderer/highlight.ts`）。
- 数学：**KaTeX v0.16 + mhchem**（`rules/katex.ts`）。
- 图表：**Mermaid v11**（`rules/mermaid.ts`）。

---

## 8. 数据源读取要点

### 8.1 本地文件夹（`file-api-driver-local.ts`）
- 列目录：`readDirStats` → 文件名 + `mtime`（秒级）。
- 读文件：直接读 `<root>/<id>.md` 文本、`<root>/.resource/<id>` 二进制。

### 8.2 WebDAV（`file-api-driver-webdav.js`, `WebDavApi.ts`）
只读只需两个 HTTP 方法：
- **PROPFIND**（`Depth: 1` 列目录 / `Depth: 0` 取单文件元数据）：请求体见下，从响应里取 `d:href` 与 `d:getlastmodified`（RFC1123 时间）。
- **GET**：下载 `.md` / 资源二进制 / `info.json`。
- 认证：**HTTP Basic Auth**，`Authorization: Basic base64(user:pass)`（`WebDavApi.ts:110-121`）。
- 不需要 PUT/DELETE/MKCOL/MOVE。

PROPFIND 请求体（`WebDavApi.ts:272-296`）：
```xml
<?xml version="1.0" encoding="UTF-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop><d:getlastmodified/><d:resourcetype/></d:prop>
</d:propfind>
```
> 解析响应注意命名空间大小写（`d:`/`D:`/默认 ns），`d:resourcetype/d:collection` 表示目录。

### 8.3 增量同步（`file-api.ts:477-653` `basicDelta`）
Joplin 用通用算法：列出全部文件的 `updated_time(mtime)`，与上次记录对比得出 新增/修改/删除。**没有服务端 delta 接口**，本质是"全量列目录 + 时间戳比对"。

> **jasper 策略**：首次全量拉取并解析 → 写入本地 SQLite（记录 `id` + `updated_time`）；之后启动先读 SQLite 秒开，后台再列目录比对 `updated_time`，只重新 GET 变更/新增的条目，删除本地缺失的。

---

## 9. 加密（本期不实现，但需识别）

- 是否启用：`info.json` 的 `e2ee.value`；以及每个条目的 `encryption_applied` 字段（1=已加密）。
- 加密条目的 `unserialize` 后只有少量明文字段（`id` `parent_id` `updated_time` `type_` 等）+ `encryption_cipher_text`，正文是密文（`models/BaseItem.ts:514-571`）。
- **jasper 行为**：检测到 `encryption_applied=1` 的条目 → 标记为"🔒 加密，暂不支持"，跳过渲染，不报错。（用户已确认数据为明文，此为兜底。）

---

## 10. 只读客户端实现清单（落地依据）

后端（Rust）需实现：
1. **StorageBackend trait**：`list() -> [(name, updated_time)]`、`get_text(name)`、`get_bytes(name)`；两实现：Local（fs）、WebDav（reqwest + PROPFIND/GET + Basic Auth）。
2. **条目解析器**：按 §3.2 算法解析 `.md` → `Note/Folder/Resource/Tag/NoteTag`；按 §3.3 处理时间戳与转义；按 §6 区分 markup_language。
3. **索引（SQLite）**：表带 `workspace_id` 列（预留多源）；存条目 + `updated_time`；FTS5 全文索引（title+body）。
4. **增量同步**：§8.3 策略。
5. **HTTP API**：`/api/folders` `/api/notes` `/api/notes/:id` `/api/resources/:id`(带 mime 头) `/api/search`。

前端（Svelte）需实现：
1. 三栏 UI（笔记本树 / 笔记列表 / 阅读区）。
2. markdown-it + §7.2 的 P0/P1 插件；HTML 笔记走净化直显。
3. `:/id` 资源链接改写为 `/api/resources/:id`；内部笔记链接拦截做应用内跳转。

---

## 11. 关键源码索引

| 主题 | 文件 |
|---|---|
| 序列化/解析 | `packages/lib/models/BaseItem.ts:401-632` |
| 文件名规则 | `packages/lib/models/BaseItem.ts:167-183` |
| type_ 枚举 | `packages/lib/BaseModel.ts:12-29` |
| 实体字段 | `packages/lib/services/database/types.ts` |
| markup_language | `packages/renderer/types.ts:3-7` |
| 资源二进制路径 | `packages/lib/services/synchronizer/utils/resourceRemotePath.ts` |
| 资源文件名 | `packages/lib/models/utils/resourceUtils.ts:8` |
| info.json | `packages/lib/services/synchronizer/syncInfoUtils.ts:278-362` |
| 本地 driver | `packages/lib/file-api-driver-local.ts` |
| WebDAV driver | `packages/lib/file-api-driver-webdav.js`, `packages/lib/WebDavApi.ts` |
| 增量算法 | `packages/lib/file-api.ts:477-653` |
| markdown 渲染入口 | `packages/renderer/MdToHtml.ts` |
| 资源链接解析 | `packages/renderer/urlUtils.js` |
| checkbox 规则 | `packages/renderer/MdToHtml/rules/checkbox.ts` |
