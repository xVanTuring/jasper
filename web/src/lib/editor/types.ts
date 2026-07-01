// 编辑器工具的抽象：句柄(EditorHandle) + 命令(EditorCommand)。
// 命令声明式（id/图标/文案/所属模式/run），不感知底层引擎；具体引擎实现 EditorHandle。
// 这层刻意对齐 plugin-design.md 的 contributes.command + contributes.toolbar{location}，
// 将来插件可用同一个 registerEditorCommand 贡献工具，宿主按 modes 过滤后渲染到 note-toolbar。

import type { BlockKind } from './markdown'

export type EditorMode = 'source' | 'wysiwyg'

// 由具体引擎（源码 CodeMirror / 富文本 Crepe）实现；命令通过它操作当前编辑器。
export interface EditorHandle {
	readonly mode: EditorMode
	focus(): void
	getValue(): string
	setValue(md: string): void
	// 行内包裹：选区两侧加 marker（如 `**`）；无选区则插占位并选中；已包裹则去除（切换）。
	wrapInline(marker: string, placeholder?: string): void
	// 任意前后缀包裹（如链接 `[text](url)`），不做切换。
	wrapAround(before: string, after: string, placeholder?: string): void
	// 对选区所在行切换块级前缀（标题/引用/列表/待办）。
	applyBlock(kind: BlockKind): void
	// 光标处插入片段（替换选区）。
	insert(text: string): void
}

export interface EditorCommand {
	id: string
	icon: string
	title: string // i18n key
	group: 'block' | 'inline' | 'insert' | 'action'
	modes: EditorMode[] // 命令适用的编辑模式；工具栏据此过滤
	run(handle: EditorHandle): void
}
