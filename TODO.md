# TODO

未完成功能清单，整理自 `CLAUDE.md`「路线 / TODO」节与 `docs/plugin-design.md` §11。核对时间：2026-07-04（对照当前源码，见下方每项的验证方式）。

## 1. LAN 鉴权 / 访问口令 ✅ 已完成（2026-07-03，访问鉴权 / 访问控制）

- 已实现：`server/src/auth.rs` 的容器级访问密码（设置页配置，加盐迭代 SHA-256 存 `config.db`）+ 会话 token（`Authorization: Bearer`，内存态）+ `guard_auth` 中间件（写操作与机密读的门控 + `Extension<Access>`）+ 读路径按 `Scope`（无密码阅读总开关 + 笔记本黑白名单子树）过滤，覆盖 folders/notes/search/detail 及标签读端点（`tags_list`/`tag_notes`/`note_tags_list`，已核对 `server/src/api.rs` 均带 `Scope` 过滤）；前端 `AuthDialog.svelte` + 统一 `readOnly` 闸门 + 设置页「访问控制」段。详见 `CLAUDE.md`「访问鉴权 / 访问控制」节。
- 已知权衡（非阻塞，见 CLAUDE.md 同节「已知权衡」）：会话仅内存态（重启需重登）。

## 2. 资源二进制访问控制（ACL）补齐 ✅ 已完成（2026-07-04，资源二进制访问控制补齐）

- 已实现：`core::library` 新增 `resource_notes` 反向索引（`build_indexes()` 里据笔记正文 `:/<id>` 引用构建，笔记增删改已有的 `build_indexes()` 调用自动保鲜）+ `notes_referencing_resource`；`server/src/api.rs::resource` handler 接入 `Extension<Access>` + `Scope`——resource→note→folder 权限链路：找到引用该资源的笔记，取其 `parent_id` 过 `scope.allows_folder`，任一可见即放行（多笔记引用取并集，语义同标签视图）；孤儿资源（无笔记引用）在受限范围下判定不可见；`Scope::All`（未设密码/已登录/passwordless+none）零开销跳过检查。详见 `CLAUDE.md`「访问鉴权 / 访问控制」节与「路线 / TODO」的完成记录。
- 测试：`core/src/library.rs` 的 `notes_referencing_resource_finds_cross_folder_refs_and_empty_for_orphan`；`server/src/api.rs` 的 `anonymous_resource_acl_scopes_by_referencing_note_folder`（白名单/黑名单/passwordless 关/孤儿资源/多笔记引用并集/Full 不受限，均已跑过）。

## 3. 标签视图 + 打标签 ✅ 已完成（2026-07-03，浏览 + 读写，兼容 Joplin）

**浏览（只读）**
- 后端：`core::library` 新增 `notes_by_tag` 索引（据 `note_tags` 关联表构建，剔除悬挂到已删笔记的关联、按 `(tag,note)` 去重）+ `tags_sorted`/`tag_note_count`/`notes_with_tag`；`server/src/api.rs` 挂 `GET /api/tags`（含篇数，按标题排序）与 `GET /api/tags/{id}/notes`（按更新时间倒序）。
- 前端：`web/src/lib/TagList.svelte` 侧栏标签区（笔记本树下方，`tag` 图标）；`App.svelte` 加 `selectedTagId`（与 `selectedFolderId` 互斥）、标签模式的列表刷新/搜索清空恢复/新建落未分类等分支；`api.ts` 加 `tags()`/`notesByTag()`（demo 库无标签 → 空）。

**打标签（读写，兼容 Joplin）**
- 序列化：`core::serialize::new_tag_md`（type_=5）/`new_note_tag_md`（type_=6）字段集与顺序**逐字对齐 Joplin 真实数据**（含空 `parent_id`/`user_data`，无 `deleted_time`；note_tag 纯元数据无标题）。字节格式已用真实 JopinData 对照 + 落盘后清缓存重解析往返验证。
- 语义对齐 Joplin `Tag`：打标签按标题 **trim + 不区分大小写复用**已有标签，不存在则新建；笔记已有该标签则幂等；去标签只删 `note_tag` 关联、**保留标签本身**（孤儿标签，同 Joplin `removeNote`）。
- API：`GET /api/notes/{id}/tags`、`POST /api/notes/{id}/tags {title}`、`DELETE /api/notes/{id}/tags/{tag_id}`（写操作受只读守卫拦截）。SSE 新增 `kind:"tag"` 事件（id=受影响笔记）。
- 前端：`NoteTags.svelte`（笔记头部标签行：chips + 移除 × + 「添加标签」输入 + 已有标签 datalist 补全），接进 `NoteView`；`App.svelte` 打标签后刷新侧栏标签区、SSE `tag` 事件驱动跨端刷新。
- 说明：尚未做**全局改标签名 / 删标签**（需改写/删 `tag` 条目 + 级联 `note_tag`）——如需再另开一项。

## 4. E2EE 解密（按需，低优先级）

- 现状：项目定位明确**不实现** Joplin 的端到端加密解密；加密条目按兜底处理（不可读）。
- 需要：暂无强需求，仅在用户明确要求时才考虑；工作量大（需对齐 Joplin 的加密协议）。

## 5. 插件系统阶段 4 —— 编辑钩子 + 扩展 widget 词汇表

详见 `docs/plugin-design.md` §11「分阶段路线」第 4 条：

- **编辑期钩子**：目前只有 `before_save`（保存时触发）；阶段 4 要加"输入时检测"钩子（编辑器打字过程中触发插件逻辑，例如实时校验/联想），需要设计新的 ABI 方法 + 前端编辑器接入点（`Editor.svelte` / `WysiwygEditor.svelte`）。
- **扩展 widget 词汇表**：当前 `UiWidget.svelte` 支持 6 种 widget（chat/list/tree/form/markdown/button），阶段 4 计划按需再加。

> 注：`docs/plugin-design.md` §11 原文括注"远程 URI / 市场后置"，但插件市场（`market.ts`/`market.svelte.ts` + registry 仓库）已在「插件生态 + 市场」阶段完成，此处该括注已过时，不算未完成项。

## 6. 仍待定的小决策（不阻塞，见 `docs/plugin-design.md` §12）

- [ ] 插件按钮"显示图标 / 文字 / 两者"是否开放给用户在设置页切换（`ui.svelte.ts` 的 store 已就绪，只差一个开关 UI）。

---

已完成的功能不在此列，完整列表见 `CLAUDE.md`「路线 / TODO」节顶部的"已完成"部分（本地+WebDAV 读写、增量缓存、资源管理、单文件打包、GHCR 发布、多语言、WYSIWYG 编辑器、拖拽移动、只读模式、插件系统阶段 1-3 + 市场、SSE 自动刷新等）。
