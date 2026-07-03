# TODO

未完成功能清单，整理自 `CLAUDE.md`「路线 / TODO」节与 `docs/plugin-design.md` §11。核对时间：2026-07-03（对照当前源码，见下方每项的验证方式）。

## 1. LAN 鉴权 / 访问口令

- 现状：服务可绑 `0.0.0.0` 暴露到局域网/容器（`JASPER_HOST`），但**没有任何身份校验**——`server/src/api.rs` 里只有 `guard_read_only` 一个中间件（按 HTTP 方法拦截写操作），没有 `Authorization`/session/口令校验。
- 需要：单一访问口令 + 会话（如 cookie），非 `127.0.0.1` 场景下防止局域网内任意人读写。
- 关联：Docker 一节已提示「当前未做鉴权，容器 0.0.0.0 暴露时谨慎」。

## 2. 标签视图 + 打标签 ✅ 已完成（2026-07-03，浏览 + 读写，兼容 Joplin）

**浏览（只读）**
- 后端：`core::library` 新增 `notes_by_tag` 索引（据 `note_tags` 关联表构建，剔除悬挂到已删笔记的关联、按 `(tag,note)` 去重）+ `tags_sorted`/`tag_note_count`/`notes_with_tag`；`server/src/api.rs` 挂 `GET /api/tags`（含篇数，按标题排序）与 `GET /api/tags/{id}/notes`（按更新时间倒序）。
- 前端：`web/src/lib/TagList.svelte` 侧栏标签区（笔记本树下方，`tag` 图标）；`App.svelte` 加 `selectedTagId`（与 `selectedFolderId` 互斥）、标签模式的列表刷新/搜索清空恢复/新建落未分类等分支；`api.ts` 加 `tags()`/`notesByTag()`（demo 库无标签 → 空）。

**打标签（读写，兼容 Joplin）**
- 序列化：`core::serialize::new_tag_md`（type_=5）/`new_note_tag_md`（type_=6）字段集与顺序**逐字对齐 Joplin 真实数据**（含空 `parent_id`/`user_data`，无 `deleted_time`；note_tag 纯元数据无标题）。字节格式已用真实 JopinData 对照 + 落盘后清缓存重解析往返验证。
- 语义对齐 Joplin `Tag`：打标签按标题 **trim + 不区分大小写复用**已有标签，不存在则新建；笔记已有该标签则幂等；去标签只删 `note_tag` 关联、**保留标签本身**（孤儿标签，同 Joplin `removeNote`）。
- API：`GET /api/notes/{id}/tags`、`POST /api/notes/{id}/tags {title}`、`DELETE /api/notes/{id}/tags/{tag_id}`（写操作受只读守卫拦截）。SSE 新增 `kind:"tag"` 事件（id=受影响笔记）。
- 前端：`NoteTags.svelte`（笔记头部标签行：chips + 移除 × + 「添加标签」输入 + 已有标签 datalist 补全），接进 `NoteView`；`App.svelte` 打标签后刷新侧栏标签区、SSE `tag` 事件驱动跨端刷新。
- 说明：尚未做**全局改标签名 / 删标签**（需改写/删 `tag` 条目 + 级联 `note_tag`）——如需再另开一项。

## 3. E2EE 解密（按需，低优先级）

- 现状：项目定位明确**不实现** Joplin 的端到端加密解密；加密条目按兜底处理（不可读）。
- 需要：暂无强需求，仅在用户明确要求时才考虑；工作量大（需对齐 Joplin 的加密协议）。

## 4. 插件系统阶段 4 —— 编辑钩子 + 扩展 widget 词汇表

详见 `docs/plugin-design.md` §11「分阶段路线」第 4 条：

- **编辑期钩子**：目前只有 `before_save`（保存时触发）；阶段 4 要加"输入时检测"钩子（编辑器打字过程中触发插件逻辑，例如实时校验/联想），需要设计新的 ABI 方法 + 前端编辑器接入点（`Editor.svelte` / `WysiwygEditor.svelte`）。
- **扩展 widget 词汇表**：当前 `UiWidget.svelte` 支持 6 种 widget（chat/list/tree/form/markdown/button），阶段 4 计划按需再加。

> 注：`docs/plugin-design.md` §11 原文括注"远程 URI / 市场后置"，但插件市场（`market.ts`/`market.svelte.ts` + registry 仓库）已在「插件生态 + 市场」阶段完成，此处该括注已过时，不算未完成项。

## 5. 仍待定的小决策（不阻塞，见 `docs/plugin-design.md` §12）

- [ ] 插件按钮"显示图标 / 文字 / 两者"是否开放给用户在设置页切换（`ui.svelte.ts` 的 store 已就绪，只差一个开关 UI）。

---

已完成的功能不在此列，完整列表见 `CLAUDE.md`「路线 / TODO」节顶部的"已完成"部分（本地+WebDAV 读写、增量缓存、资源管理、单文件打包、GHCR 发布、多语言、WYSIWYG 编辑器、拖拽移动、只读模式、插件系统阶段 1-3 + 市场、SSE 自动刷新等）。
