# jasper 插件规范 v0.1（草案）

> 状态：**草案，征求确认**。这是给「插件作者」和「宿主实现」照着做的**契约**；方案/路线见 [plugin-design.md](./plugin-design.md)。
> 规范用词遵循 RFC 2119：**MUST/必须**、**SHOULD/应当**、**MAY/可以**。
> 本版 `apiVersion = "0.1"`。破坏性改动升 MAJOR，向后兼容的新增升 MINOR。

---

## 0. 范围与术语

- **宿主（host）**：jasper 本体（server + web）。
- **插件（plugin）**：一个 zip 包，向宿主贡献以下一种或多种：**主题**、**后端逻辑（wasm）**、**前端贡献**、**设置**。
- **后端模块**：插件里的 `*.wasm`，用 Rust 编译，跑在宿主的 **wasmi** 沙箱里。
- **能力（capability）**：插件访问宿主资源的授权项，**默认全部拒绝**。
- 时间戳一律 **Unix 毫秒**（i64）。字段命名一律 **snake_case**（与现有 HTTP API、jasper-core 一致）。

---

## 1. 插件类型

一个插件由 `manifest.toml` 声明它贡献什么，不是互斥的“类型”：

| 贡献 | 需要 wasm | 典型例子 |
|---|---|---|
| **主题** `[[contributes.theme]]` | 否（纯 CSS） | 换肤、换图标 |
| **后端钩子/命令** `[backend]` | 是 | 保存前格式化、AI 编排 |
| **前端贡献** `[[contributes.*]]` | 看情况 | 侧边栏面板、工具栏按钮 |
| **设置** `[settings.schema]` | 否 | 插件配置项 |

- **纯主题插件 MUST NOT 含 wasm**，宿主按“零代码”信任档加载（见 §11）。
- 含 `[backend]` 的插件 MUST 提供合法 wasm 且声明所需 capabilities。

---

## 2. 包格式与目录布局

- 一个插件是一个 **zip**（建议扩展名 `.jplug`，宿主 MUST 同时接受 `.zip`）。
- 解包后根目录布局：

```
manifest.toml          # 必须，根目录
plugin.wasm            # 含 [backend] 时必须；路径由 [backend].wasm 指定
assets/                # 可选：css / svg / 图片等
  themes/<id>.css
```

- 宿主安装时 MUST 把包解到 `<config>/plugins/<id>/`（`<id>` = manifest 的 `id`）。
- 路径字段（`wasm`、`css` 等）MUST 为相对包根的正斜杠路径，MUST NOT 逃出包根（禁止 `..`、绝对路径）。
- 当前仅支持**本地加载**（放目录 / 上传 zip）；远程 URI / 市场后置（见 design §4）。

---

## 3. manifest.toml 规范

宿主本版**仅接受 `manifest.toml`**（不接受 JSON）。未知字段 MUST 被忽略（向前兼容）。

### 3.1 顶层字段

| 字段 | 类型 | 必须 | 约束/说明 |
|---|---|---|---|
| `id` | string | ✅ | `^[a-z0-9][a-z0-9-]*$`，全局唯一，即安装目录名 |
| `name` | string | ✅ | 显示名 |
| `version` | string | ✅ | semver（如 `1.2.0`） |
| `apiVersion` | string | ✅ | 插件面向的插件 API 版本，`MAJOR.MINOR`（本版 `0.1`） |
| `description` | string | ⬜ | 一句话简介 |
| `author` | string | ⬜ | |
| `minHostVersion` | string | ⬜ | 要求的最低宿主版本（semver） |

### 3.2 `[backend]`（可选）

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `wasm` | string | — | wasm 路径；本表存在则**必须** |
| `capabilities` | array<string> | `[]` | 申请的能力（§7），未申请即拒绝 |
| `hooks` | array<string> | `[]` | 订阅的钩子（§8），值取自钩子名 |

### 3.3 `[[contributes.theme]]`（数组，可多个）

| 字段 | 类型 | 必须 | 说明 |
|---|---|---|---|
| `id` | string | ✅ | 主题 id（`data-theme` 的值），同插件内唯一 |
| `name` | string | ✅ | 显示名 |
| `base` | `"light"`\|`"dark"` | ✅ | 决定未覆盖令牌的回退基调 + 编辑器明暗 |
| `css` | string | ✅ | 主题 CSS 路径（相对包根） |

### 3.4 `[[contributes.command]]`

| 字段 | 类型 | 必须 | 说明 |
|---|---|---|---|
| `id` | string | ✅ | 命令 id（插件内唯一） |
| `title` | string | ✅ | 显示名 |
| `target` | `"backend"`\|`"builtin"` | ✅ | backend=调 wasm `command` 方法；builtin=宿主内置动作 |
| `icon` | string | ⬜ | 图标令牌名（§9.1），不带 `--icon-` 前缀 |

### 3.5 `[[contributes.sidebar]]`

| 字段 | 类型 | 必须 | 说明 |
|---|---|---|---|
| `id` | string | ✅ | 面板 id |
| `title` | string | ✅ | |
| `icon` | string | ⬜ | 图标令牌名 |
| `widget` | string | ✅ | 用哪个内置 widget（§9.2） |

### 3.6 `[[contributes.toolbar]]`

| 字段 | 类型 | 必须 | 说明 |
|---|---|---|---|
| `command` | string | ✅ | 指向某 `contributes.command.id` |
| `location` | `"note-toolbar"`\|`"topbar"` | ✅ | 放置位置 |

### 3.7 `[[contributes.editor]]`

| 字段 | 类型 | 必须 | 说明 |
|---|---|---|---|
| `on` | `"before-save"`\|`"input"` | ✅ | before-save=保存前；input=debounce 输入时 |
| `command` | string | ⬜ | 可选，复用某命令；缺省调 `editor.transform`（§8） |

### 3.8 `[settings.schema]`

每个键是一个设置项，值是字段定义（见 §10）。

---

## 4. 版本与兼容

- 宿主 MUST 暴露自身支持的 `apiVersion` 集合（如 `["0.1"]`）。
- 加载时：若 `manifest.apiVersion` 的 MAJOR 不在宿主支持集，MUST 拒绝并提示。MINOR 更高可加载但宿主 MAY 警告。
- `minHostVersion` 大于当前宿主版本时 MUST 拒绝。
- 同 `id` 重复安装：以更高 `version` 覆盖；相等或更低 MUST 询问/拒绝。

---

## 5. 生命周期

```
发现(扫描 plugins 目录 / 上传)
  → 读 manifest → 校验(§3/§4) → 失败则标记错误并跳过
  → [若有 backend] 编译 wasm(wasmi) → 用“受能力门控的 import”实例化
  → 调 dispatch:"metadata" 握手(可选)
  → 注册贡献(主题/命令/面板/钩子/设置)
  → enabled
启停: disable 后宿主 MUST NOT 再调其钩子/命令；enable 恢复
卸载: 释放实例，移除注册；删除 = 卸载 + 删目录
```

- 后端实例化策略：宿主 MUST 为**每次调用新建实例**（状态隔离，不跨调用共享可变状态）。单次调用 MUST 施加资源上限（§11）。
- 钩子/命令调用 MUST 在 server 的 `spawn_blocking` 中执行（不阻塞异步运行时）。

---

## 6. 后端 ABI（wasm，Rust-only）

编码一律 **UTF-8 JSON**。指针/长度均 `u32`，打包返回值 `u64 = (ptr << 32) | len`。

### 6.1 内存与分配（插件 MUST 导出）

- `memory`：线性内存。
- `plugin_alloc(size: u32) -> u32`：返回可写 `size` 字节的指针。
- `plugin_free(ptr: u32, size: u32)`：释放。

### 6.2 host → 插件：`plugin_dispatch`（插件 MUST 导出）

```
plugin_dispatch(ptr: u32, len: u32) -> u64
```
- 入参：`ptr/len` 指向插件内存里的**请求 JSON**：`{ "method": string, "params": <json> }`。
- 返回：打包的 `(out_ptr, out_len)`，指向插件内存里的**响应 JSON**（§6.4）。宿主读完 MUST 调 `plugin_free` 释放。

### 6.3 插件 → host：`host_call`（宿主 MUST 提供为 import，模块名 `joplin`）

```
joplin.host_call(ptr: u32, len: u32) -> u64
```
- 插件把请求 JSON 写进自己的内存，传 `ptr/len`。
- 宿主读取 → 鉴权（§7）→ 执行 → 通过回调插件的 `plugin_alloc` 把**响应 JSON** 写入插件内存，返回打包 `(out_ptr, out_len)`。插件读完 MUST 调 `plugin_free`。
- 该 import 是插件访问宿主的**唯一**通道；具体能做什么由 capabilities 决定。

### 6.4 JSON 信封与错误模型

请求：
```json
{ "method": "notes.get", "params": { "id": "…" } }
```
响应（二选一）：
```json
{ "ok": true,  "result": { … } }
{ "ok": false, "error": { "message": "…", "code": "forbidden" } }
```
- `code` 取值（约定）：`forbidden`(能力不足) / `not_found` / `invalid` / `internal` / `unsupported`。
- 插件返回非法 JSON、越界指针、或超资源上限：宿主 MUST 视该调用失败（`internal`），不得使整库崩溃。

> **SDK**：`plugin-sdk`（Rust）封装上述 ABI，作者只写 `#[on_before_save] fn(note: Note) -> Result<Note>` 之类；共享 `jasper-core` 的 `Note/Folder` 类型，serde 直接序列化。

### 6.5 方法清单

**host → 插件（plugin_dispatch 的 method）**

| method | params | result | 触发 |
|---|---|---|---|
| `metadata` | — | `{ ok: true }` | 握手（可选） |
| `hook.before_save` | `{ note }` | `{ note }` | 订阅 `hooks=["before-save"]`，保存前 |
| `command` | `{ id, args? }` | `{ result? , ui? }` | 命令被触发 |
| `editor.transform` | `{ phase, text }` | `{ text }` | `contributes.editor`，`phase`∈`before-save\|input` |
| `ui` | `{ view, state? }` | `UiNode`（§9.3） | 动态面板取声明树 |

**插件 → host（host_call 的 method，括号为所需能力）**

| method | params | result | 能力 |
|---|---|---|---|
| `log` | `{ level, message }` | `{}` | 无 |
| `notes.get` | `{ id }` | `{ note }` | `notes:read` |
| `notes.search` | `{ query, limit? }` | `{ notes: NoteRef[] }` | `notes:read` |
| `notes.list_folders` | — | `{ folders: FolderRef[] }` | `notes:read` |
| `notes.upsert` | `{ id, title?, body? }` | `{ note }` | `notes:write` |
| `notes.create` | `{ parent_id, title?, body? }` | `{ note }` | `notes:write` |
| `ai.complete` | `{ messages, options? }` | `{ content }` | `host:ai` |
| `settings.get` | `{ key }` | `{ value }` | `settings` |
| `settings.set` | `{ key, value }` | `{}` | `settings` |

**数据形状**
```
Note      = { id, title, body, parent_id, markup_language, created_time,
              updated_time, is_todo, todo_completed, source_url }
NoteRef   = { id, title, parent_id }
FolderRef = { id, title, parent_id }
Message   = { role: "system"|"user"|"assistant", content: string }
```
- `markup_language`：1=Markdown，2=HTML。
- `notes.upsert` 仅改传入的字段；其余元数据宿主**逐字保留**（复用 `serialize::update_note_md`）。

---

## 7. 能力（capabilities）

默认全拒。manifest `[backend].capabilities` 申请，安装时向用户展示并征得同意。

| 能力 | 解锁的 host 方法 | 说明 |
|---|---|---|
| `notes:read` | `notes.get` / `notes.search` / `notes.list_folders` | 只读 |
| `notes:write` | `notes.upsert` / `notes.create` | **默认每次写入弹 UI 确认**；用户可在该插件设置里开“免确认” |
| `host:ai` | `ai.complete` | 密钥/端点在宿主，插件不可见 |
| `settings` | `settings.get` / `settings.set` | 插件作用域 KV |

- `log` 不需能力。
- `host:fetch`（任意网络出口）**本版不提供**。
- 调用未授权方法 MUST 返回 `{ok:false, error:{code:"forbidden"}}`。

---

## 8. 钩子与扩展点

| 钩子名（hooks 值） | dispatch method | 语义 |
|---|---|---|
| `before-save` | `hook.before_save` | 笔记保存前改写；返回的 `note` 写回。多个插件按加载顺序串联。 |

其它扩展点不靠 `hooks`，靠 `contributes.*` 声明触发：
- **命令**：`contributes.command` + 触发（工具栏/面板）→ `command` 方法。
- **编辑器**：`contributes.editor` → `editor.transform` 方法（`before-save` 作用于 body，`input` 作用于 debounce 后文本）。

后置（本版不实现，预留方法名）：`render_fence`（自定义代码块渲染）、`import`/`export`。

钩子约束：
- `hook.before_save` 返回错误时，宿主 MUST 放弃该插件的改写、用原 note 继续保存（**不因插件失败而丢用户数据**），并提示。

---

## 9. 前端贡献

### 9.1 主题：CSS 令牌契约（已实现，本节为权威清单）

主题 = 一份 CSS，覆盖以下令牌；选择器 MUST 为 `:root[data-theme='<主题id>'] { … }`。未覆盖的令牌回退到 `base` 基调默认值。
本清单为 v0.1 **冻结契约、只增不删**：新增令牌升 MINOR；删除/改名/改语义算破坏性，升 MAJOR。

**颜色语义令牌（可覆盖）**
```
--bg --bg-bar --bg-side
--text --text-dim
--border --hover
--accent --accent-soft
--code-bg
--danger --danger-soft --danger-soft-weak
--success
--on-accent          /* 强色按钮上的文字色 */
--overlay            /* 弹层遮罩 */
--shadow-modal       /* 弹层阴影（完整 box-shadow 值） */
--code-block-bg --code-block-text
```

**图标令牌（可覆盖，值为 `url("data:image/svg+xml,…")`）**
```
--icon-close --icon-plus --icon-settings --icon-image --icon-folder
--icon-file --icon-alert --icon-edit --icon-trash --icon-eye
--icon-code --icon-rich --icon-attach --icon-clean --icon-cloud
--icon-globe --icon-sun --icon-moon --icon-contrast --icon-palette
```
- 图标经 CSS `mask` + `currentColor` 渲染：**单色、跟随文字色**；SVG 颜色无意义，只取 alpha。
- 主题 SHOULD NOT 依赖未在此列出的内部 class 选择器（不稳定，不保证兼容）。
- `--palette-*`（基础调色板）是实现细节，主题 MAY 覆盖但不在稳定契约内。

### 9.2 widget 词汇表

插件**只声明用哪个 widget + 给数据**，不写 HTML/JS/CSS。本版词汇表：

| widget | 用途 | 关键 props |
|---|---|---|
| `chat` | 消息流 + 输入框（AI 面板） | `messages: Message[]`, `placeholder?` |
| `list` | 列表 | `items: {id,title,subtitle?,icon?}[]` |
| `tree` | 树 | `nodes:{id,title,children?}[]` |
| `form` | 表单（由 schema 或后端字段渲染） | `fields`(§10), `values` |
| `markdown` | 渲染一段 markdown/HTML（经 DOMPurify） | `source: string` |
| `button` | 动作按钮 | `label`, `icon?`, `command` |

未来按需扩词汇表（升 MINOR）。

### 9.3 server-driven UI 声明树

动态面板（`dispatch:"ui"`）返回一棵 **UiNode**：
```
UiNode = {
  "type": string,                 // widget 名（§9.2）
  "props": { … },                 // 该 widget 的 props
  "children": UiNode[]            // 可选
}
```
- 节点内的交互通过 `props.command`（指向 `contributes.command`）回调后端。
- 宿主 MUST 只渲染已知 widget；未知 `type` MUST 安全忽略（不报错、不注入）。

### 9.4 命令 / 侧边栏 / 工具栏 / 设置贡献

- **侧边栏**：`contributes.sidebar` 在左栏加入口，点开渲染指定 `widget`（静态）或 `ui` 方法返回的树（动态）。
- **工具栏**：`contributes.toolbar` 把某命令放到 `note-toolbar` 或 `topbar`。
- **命令**：`target="backend"` 调后端 `command`；`target="builtin"` 触发宿主内置动作（白名单，TBD）。
- 显示图标/文字由全局按钮显示模式（已实现 `ui.svelte.ts` 的 `both|icon|text`）统一控制。

---

## 10. 设置 schema

`[settings.schema]` 每项：
```toml
[settings.schema]
api_key = { type = "secret", label = "API Key" }
model   = { type = "string", default = "claude-opus-4-8" }
auto_approve_write = { type = "bool", default = false, label = "AI 改笔记免确认" }
provider = { type = "select", options = ["claude", "openai"], default = "claude" }
```

| `type` | 渲染 | 值类型 |
|---|---|---|
| `string` | 单行输入 | string |
| `multiline` | 多行 | string |
| `secret` | 密码框（值在宿主存储；后端经 `settings.get` 读，前端**不回显**） | string |
| `bool` | 开关 | bool |
| `number` | 数字 | number |
| `select` | 下拉（需 `options`） | string |

公共可选键：`label`、`default`、`description`。值存于宿主、**按插件 id 隔离**。

---

## 11. 安全与资源限制

**信任分档**
- 纯主题（CSS）/ widget 声明：低风险，默认放行。
- 后端 wasm：沙箱 + 能力白名单 + 密钥不下放；安装时 MUST 展示申请的 capabilities 并征得同意。
- **不支持任意前端 JS / iframe**（本版）。

**沙箱与上限（后端每次调用）**
- 默认零 WASI 能力（无 fs/socket/clock，除非未来显式授权）。
- 宿主 MUST 施加：**fuel 上限**（默认 ~1e9 指令）、**线性内存上限**（默认 64 MiB）、**墙钟超时**（默认 2s），均可配置。超限 MUST 中止该调用并记错。
- 宿主 MAY 在插件反复超限/报错后自动停用它。

**写入与外发**
- `notes:write` 默认 UI 二次确认（§7）。
- `ai.complete` 的密钥/端点由宿主托管，插件不可见；宿主 SHOULD 对外发目标做 allowlist。

**主题 CSS**
- SHOULD NOT 含远程 `@import` / 远程 `url()`（防外联探测）；宿主 MAY 在加载时净化/拒绝。

---

## 12. v0.1 决策（已冻结，2026-06-30）

1. **包扩展名**：`.jplug`（宿主同时接受 `.zip`）。
2. **后端实例**：每次调用新建实例（状态隔离）。
3. **资源上限默认**：内存 64 MiB、墙钟 2s、fuel ~1e9 指令，均可配置。
4. **`ai.complete`**：本版一次性返回；流式（增量）后置——以后作为可选能力新增，不改现有签名。
5. **manifest**：仅 `manifest.toml`，不接受 JSON。
6. **主题契约**：§9.1 令牌清单为权威、**只增不删**（删/改 = 破坏性，升 MAJOR）。

`apiVersion 0.1` 据此冻结；后续实现（`plugin-sdk` + 宿主加载器）照本规范。

---

## 附录 A：最小主题插件

```
my-theme.jplug
├── manifest.toml
└── assets/themes/oceanic.css
```
```toml
# manifest.toml
id = "oceanic"
name = "Oceanic"
version = "1.0.0"
apiVersion = "0.1"

[[contributes.theme]]
id = "oceanic"
name = "Oceanic Dark"
base = "dark"
css = "assets/themes/oceanic.css"
```
```css
/* assets/themes/oceanic.css */
:root[data-theme='oceanic'] {
  --bg: #0f1b2d;
  --text: #cdd9e5;
  --accent: #4fb6c2;
  --border: #1e3047;
  /* 顺手换个图标 */
  --icon-settings: url("data:image/svg+xml,%3Csvg…%3E");
}
```

## 附录 B：最小后端插件（保存前格式化）

```toml
# manifest.toml
id = "trim-trailing"
name = "去行尾空白"
version = "0.1.0"
apiVersion = "0.1"

[backend]
wasm = "plugin.wasm"
capabilities = []          # 纯计算，不碰笔记库/AI
hooks = ["before-save"]
```
```rust
// 用 plugin-sdk（伪代码）：作者只写业务，ABI/JSON 由 SDK 处理
use joplin_plugin_sdk::*;
use jasper_core::model::Note;

#[on_before_save]
fn before_save(mut note: Note) -> Result<Note, String> {
    note.body = note.body.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(note)
}
```
