# joplin-lite

轻量级 Joplin 兼容客户端：**本地 Rust(axum) 服务 + 浏览器 SPA**，不依赖 Electron/Tauri/WebView。
目标：启动快、内存小（后端常驻 ~10MB）、跨平台（macOS/Windows/Linux）、可读可写。

> 注意：仓库根目录下的 `joplin/` 是**上游 Joplin v3.6.15 源码（仅作格式参考，自带独立 git，已 gitignore）**；
> `JopinData/` 是**用户真实笔记测试数据（隐私，已 gitignore，勿提交/勿误删/勿在其上做破坏性测试）**。
> 本项目代码在 `core/`（纯逻辑）、`server/`（原生后端）、`web/`（前端）、`wasm/`（浏览器 demo）。
> 三个 Rust crate 用 path 依赖串联，**无 workspace**（`cd server && cargo run` 照常）。

## 目录结构

```
core/          纯逻辑 crate (joplin-core)：无 IO，可编译到 WASM
  src/
    model.rs       Note/Folder/Resource/Tag/NoteTag + ItemType/MarkupLanguage 枚举
    parser.rs      解析 Joplin .md 条目（含真实数据单测）
    serialize.rs   写回：update_note_md / new_note_md / new_resource_md / new_id / ISO 时间
    library.rs     内存索引（笔记本树/笔记/资源/标签/搜索 + 增删改）；Library::from_contents
server/        Rust 后端 (axum)，依赖 joplin-core
  src/
    main.rs        启动：加载配置→建索引→挂 API + 托管前端；pub use joplin_core::{...} 重导出
    config.rs      SQLite 配置存储(ConfigStore) + build_storage 工厂 + source_key
    cache.rs       SQLite 增量缓存(CacheStore)：按数据源缓存条目原始内容+mtime
    indexer.rs     从存储拉取(rayon 并行)+增量缓存协调 → 调 Library::from_contents（IO 在此，不进 core）
    storage/       StorageBackend trait
      mod.rs         trait 定义 + is_item_filename + DEFAULT_INFO_JSON
      local.rs       本地文件夹后端
      webdav.rs      WebDAV 后端 (ureq + roxmltree, PROPFIND/GET/PUT/DELETE/MKCOL)
    api.rs         axum 路由与 handler，AppState
wasm/          浏览器 demo crate (joplin-wasm)：joplin-core + 内置演示库 → wasm-bindgen
  src/lib.rs / demo.rs   暴露 folders/notes/note/search（只读），内置纯文本演示库
web/           Svelte 5 (runes) + Vite + TS 前端
  src/
    App.svelte         三栏布局 + 状态/配置流程
    lib/
      api.ts           后端 API 客户端 + 类型
      render.ts        markdown 渲染（markdown-it + 插件 + highlight.js + KaTeX + DOMPurify）
      FolderTree.svelte / NoteList.svelte / NoteView.svelte
      Editor.svelte    CodeMirror6 懒加载编辑器
      Settings.svelte  首次配置向导 / 设置页
      messages.ts      中/英文案字典（zh 基准；`const en: typeof zh` 强制不漏键）
      i18n.svelte.ts   rune 存当前语言 + t() 取词/插值（localStorage 持久化 + 浏览器自动选）
    shims.d.ts         无类型 markdown-it 插件的最小声明
docs/joplin-data-format.md   逆向出的 Joplin 数据格式规范（解析层依据）
Dockerfile / docker-compose.yml / .dockerignore
```

## 开发与运行

```bash
# 后端（默认读已保存配置；无配置则进入“未配置态”，由前端向导配置）
cd server && cargo run                 # 监听 127.0.0.1:27583
cd server && cargo run -- <路径或URL>   # 首次引导用数据源（会存进配置）
cd server && cargo test                # 运行单元测试（parser/serialize/webdav）

# 前端
cd web && pnpm dev                     # 开发热更新(5173)，/api 代理到 27583
cd web && pnpm build                   # 产出 web/dist，由后端在 27583 直接托管

# 生产：构建前端后访问 http://127.0.0.1:27583/

# 单文件打包（前端内嵌进二进制，运行时不依赖磁盘上的 web/dist）
cd web && pnpm build                            # 必须先有 web/dist
cd server && cargo build --release --features embed   # 产物 server/target/release/joplin-lite

# 本地起一个 WebDAV 服务端联调（hacdias/webdav，端口 8081，账号 joplin/joplin）
docker compose -f docker-compose.dev.yml up -d
cd server && JOPLIN_LITE_SOURCE=http://127.0.0.1:8081/ \
  JOPLIN_LITE_WEBDAV_USER=joplin JOPLIN_LITE_WEBDAV_PASS=joplin cargo run
docker compose -f docker-compose.dev.yml down -v   # 用完清理（含数据卷）
```

环境变量（见 `server/src/main.rs` 头注释）：
- `JOPLIN_LITE_HOST`（默认 127.0.0.1；局域网/容器设 0.0.0.0）
- `JOPLIN_LITE_PORT`（默认 27583）
- `JOPLIN_LITE_CONFIG_DIR`（配置库目录；默认平台配置目录 `joplin-lite/config.db`）
- `JOPLIN_LITE_WEB_DIR`（前端静态目录；默认相对源码，容器里指向打包路径）
- 首次引导：`JOPLIN_LITE_SOURCE` / `JOPLIN_LITE_WEBDAV_USER` / `JOPLIN_LITE_WEBDAV_PASS`

## 浏览器 WASM demo（纯前端预览，无 server）

- `joplin-core`（model/parser/serialize/library）编译到 wasm，配 `wasm/` 内置纯文本演示库，做成**零服务器**的只读预览（GitHub Pages 可挂）。
- 构建：`cd web && pnpm build:demo`（= `wasm-pack build ../wasm --target web --out-dir ../web/src/wasm-pkg` + `VITE_DEMO=1 vite build`）。需先装 `rustup target add wasm32-unknown-unknown` 与 `wasm-pack`。
- 前端切换：`web/src/lib/api.ts` 里 `VITE_DEMO=1` → 只读路径走 wasm（`IS_DEMO` 导出供 UI 用）；否则照常走 HTTP。
- **不影响原生**：`DEMO=false` 时 Rollup 把 wasm import 整个 tree-shake 掉，原生构建既不打包也不依赖 `web/src/wasm-pkg`（该目录由 wasm-pack 生成、已 gitignore）。
- demo 下隐藏所有写入入口（新建/编辑/删除/设置/资源），顶部有「演示预览」横幅说明能力边界。
- 截图见 `docs/screenshots/05-wasm-demo.png`。

## 数据源与配置

- 数据源支持**本地文件夹**和 **WebDAV**，配置（含 WebDAV 密码，**明文**）存到 SQLite（rusqlite bundled）。
- 启动时：已保存配置优先；否则用命令行/环境变量引导；都没有则**未配置态**（前端弹全屏向导：现有库/新建库 × 本地/WebDAV）。
- `AppState` 用 `RwLock<Option<Arc<dyn StorageBackend>>>` 支持**运行时切换数据源**（设置页 ⚙）。
- 新建库 = `StorageBackend::init_new()`：建根目录 + `.resource/` + 写默认 `info.json`(v3)。
- **增量缓存**：配置目录下另有 `cache.db`，按 `source_key`（不含密码）隔离缓存每条 `<id>.md` 的 `mtime+原始内容`。
  启动/切换数据源时只对 `list_items()` 里新增或 mtime 变化的条目发起拉取（WebDAV 省去逐个 GET），未变的复用缓存，已删的清理。
  缓存陈旧无害（任何写入都会刷新 mtime → 下次视为变化）；`cache.db` 删除最坏退化为一次全量拉取。`AppState.cache` 持有 `CacheStore`。

## API

```
GET    /api/status            是否已配置 + 计数 + 数据源类型
GET    /api/config            当前配置
PUT    /api/config            设置/切换数据源（连接+校验+建索引+持久化）
GET    /api/folders           笔记本树（嵌套 + 篇数）
GET    /api/notes?folder=ID   笔记列表
GET    /api/notes/{id}        笔记详情
POST   /api/notes             新建笔记 { parent_id, title?, body? }
PUT    /api/notes/{id}        更新笔记 { title, body }
DELETE /api/notes/{id}        删除笔记
GET    /api/resources         资源清单（含 used_by 引用计数，孤儿在前）
POST   /api/resources         上传资源（原始二进制为体，?filename= + Content-Type=mime）→ {id,markdown,…}
GET    /api/resources/{id}    资源二进制（带 mime 头）
PUT    /api/resources/{id}    重命名资源 { title }
DELETE /api/resources/{id}    删除资源（二进制 + 元数据条目）
GET    /api/search?q=...      标题/正文全文搜索
```

## Joplin 格式要点（详见 docs/joplin-data-format.md）

- 条目文件 `<32hex>.md` 平铺在同步根目录，三段式：`标题 \n\n 正文 \n\n key:value 元数据块`。
  **解析从文件末尾往上扫**，结尾连续非空行=元数据，遇第一个空行停；标题=正文段第一行。
- 只读/写处理的 type_：1 笔记 / 2 笔记本 / 4 资源 / 5 标签 / 6 note_tag。
- 资源二进制在 `.resource/<id>`（无扩展名），mime 来自对应 type_=4 元数据条目。
- 笔记 `markup_language`：1=Markdown、2=HTML（**HTML 笔记须净化直显，不能走 markdown 渲染**）。
- 时间戳是 ISO UTC `YYYY-MM-DDTHH:mm:ss.SSSZ` ↔ Unix 毫秒。
- WebDAV 只用 PROPFIND/GET/PUT/DELETE/MKCOL + Basic Auth；忽略 locks/temp/.sync。

## 写回与 Joplin 同步的关系

- 编辑保存 = 读原 `.md` → 改标题/正文 → 刷新 `updated_time`/`user_updated_time` → 其余元数据**逐字保留** → 写回（本地原子 rename / WebDAV PUT）。`library` 缓存了笔记原始内容(raw_notes)，保存无需重新拉取。
- 写回的是**同一个数据源**，Joplin 下次同步会拾取改动；两端都改会由 Joplin 生成冲突副本。
- 本项目**不参与 Joplin 的同步锁协议**（个人使用低风险）。

## 约定与注意点（gotchas）

- Rust：tab 缩进、单引号、避免 `any`、注释用 `//`；网络/磁盘 IO 在 axum handler 里走 `spawn_blocking`；`ConfigStore`(rusqlite) 用 `Mutex` 包裹。
- 前端：Svelte 5 **runes**（`$state/$props/$derived/$effect`），事件用 `onclick=` 不是 `on:click`；`NoteView` 按笔记 id 以 `{#key}` 重挂载（先取详情再切 id）。
- **多语言**：自建轻量 i18n（无第三方包）。新增/改文案要**同时**写 `messages.ts` 的 `zh` 和 `en`（漏键编译报错）。组件里用 `t('key', {插值})`；模板内调用 `t()` 会读 `i18n.svelte.ts` 的 rune→切换语言即时重渲染。纯 `.ts`（如 api.ts）里调 `t()` 取当时语言即可。顶栏「中/EN」按钮切换，localStorage 持久化。
- **依赖哲学：少造轮子但也别堆包**（用户偏好），渲染优先用成熟 markdown-it 插件。
- markdown-it 插件存在 CJS/ESM 默认导出差异，`render.ts` 用 `P()` 助手统一（否则 `md.use()` 抛 `e.apply is not a function`，白屏）。
- 代码高亮固定用 **github-dark** 主题 + 深色代码块背景（浅色模式也清晰）。
- 资源链接 `:/id`（含笔记内嵌的原始 `<img src=":/id">`）在**最终 HTML 上用 DOMParser 统一改写**为 `/api/resources/id`，覆盖 markdown 与 HTML 笔记。
- CodeMirror 经 `import()` 懒加载，单独成 chunk，不进首屏包。

## 测试数据

`JopinData/`（用户真实数据，gitignore）：约 340 笔记 / 88 笔记本 / 135 资源 / 3 标签 / 4 note_tag，未加密。
parser 单测对其做全量解析校验（计数断言）。**写入类测试务必用临时空目录，不要在 JopinData 上做破坏性操作。**

## 单文件打包（rust-embed 内嵌前端）

- `server` 的可选 feature `embed`（`server/Cargo.toml`）开启后，用 **rust-embed** 在编译期把 `web/dist` 整个塞进二进制；运行时 axum 的 fallback 直接吐内嵌资源（`server/src/web_assets.rs`），不再依赖磁盘上的前端目录 → **单个可执行文件即完整应用**。
- 构建：`cd web && pnpm build` 后 `cd server && cargo build --release --features embed`。**必须先有 `web/dist`**，否则 rust-embed 编译期校验文件夹存在会直接报错。
- 默认（不带 `--features embed`）行为**完全不变**：开发/源码构建仍从磁盘 `../web/dist` 托管，`cargo run` 无需先构建前端也能编译（dev 期前端跑 Vite）。
- 静态托管优先级（`main.rs::attach_web`）：① `JOPLIN_LITE_WEB_DIR` 指定的磁盘目录（两种构建都支持，便于热替换前端） → ② `embed` 构建用内嵌资源 → ③ 源码旁 `../web/dist`。
- SPA 回退：未命中路径回 `index.html`（与原 `ServeDir.not_found_service` 一致）；mime 由 rust-embed 的 `mime-guess` feature 在编译期定。

## Docker

多阶段构建（node 构建前端 → rust 用 `--features embed` 把前端**内嵌**进二进制 → debian-slim 运行）。
运行镜像里**只有一个自带前端的二进制**（不再单独 COPY dist、不再设 `JOPLIN_LITE_WEB_DIR`）。配置目录挂卷 `/config` 持久化。
- 本地：`docker compose up --build`，访问 `http://localhost:27583/`。
- 发布 GHCR：`.github/workflows/docker.yml` 在推 `main`/`v*` tag/手动时构建并推到 `ghcr.io/<owner>/<repo>`（main→`latest`+`sha-…`，tag→语义化版本）；用内置 `GITHUB_TOKEN`，无需额外 secret。
- 拉取运行：`docker run -p 27583:27583 -v joplin-config:/config ghcr.io/<owner>/joplin-lite:latest`。
（注意：当前未做鉴权，容器 0.0.0.0 暴露时谨慎；网络受限地区拉取 Docker Hub 基础镜像可能需镜像加速。）

## 路线 / TODO

已完成：本地+WebDAV 读、增删改编辑（CodeMirror+自动保存）、SQLite 配置+向导、Docker 打包、
**增量缓存**（cache.db 按数据源缓存条目原始内容+mtime，启动只拉取变化项；见 cache.rs / library::build_cached）、
**资源/图片上传**（POST /api/resources 写 `.resource/<id>` 二进制 + `<id>.md`(type_=4) 元数据；编辑器粘贴/拖拽/📎 上传后插入 `:/id` 引用）、
**资源管理面板**（顶栏 🖼：清单+缩略图+引用计数、重命名、删除、一键清理孤儿；见 web/src/lib/ResourcePanel.svelte，引用计数 library::resource_usage 扫正文 `:/id`）、
**单文件打包**（`--features embed` 用 rust-embed 内嵌 web/dist；见上「单文件打包」节）、
**GHCR 发布**（`.github/workflows/docker.yml` 推 main/v* tag 时构建并推 `ghcr.io/<owner>/<repo>`）。
待办：LAN 鉴权/访问口令、标签视图、E2EE 解密（按需）。
