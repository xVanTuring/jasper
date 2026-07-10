// 编辑器命令注册表 + 内置 markdown 工具。
// 开放注册：registerEditorCommand（内置与将来插件走同一入口）；commandsForMode 按模式过滤。

import { t } from '../i18n.svelte'
import { formatMarkdown } from './markdown'
import type { EditorCommand, EditorMode } from './types'

const registry: EditorCommand[] = []

// 注册（同 id 覆盖，便于插件替换内置命令）。
export function registerEditorCommand(cmd: EditorCommand): void {
	const i = registry.findIndex((c) => c.id === cmd.id)
	if (i >= 0) registry[i] = cmd
	else registry.push(cmd)
}

// 取某模式下可用的命令（保持注册顺序）。
export function commandsForMode(mode: EditorMode): EditorCommand[] {
	return registry.filter((c) => c.modes.includes(mode))
}

// 表格插入片段（表头文案随当前语言）。
function tableSnippet(): string {
	const a = t('editor.tool.tableCol', { n: 1 })
	const b = t('editor.tool.tableCol', { n: 2 })
	return `\n| ${a} | ${b} |\n| --- | --- |\n|  |  |\n`
}

// 内置命令：统一到一个 CodeMirror 实例后，源码与 Live Preview 是同一实例的两种视图，
// 故命令对两模式通用（BOTH）。modes 仍保留为过滤维度，将来插件可注册只在某一模式出现的命令。
const BOTH: EditorMode[] = ['source', 'live']

const builtins: EditorCommand[] = [
	{ id: 'md.h1', icon: 'heading-1', title: 'editor.tool.h1', group: 'block', modes: BOTH, run: (h) => h.applyBlock('h1') },
	{ id: 'md.h2', icon: 'heading-2', title: 'editor.tool.h2', group: 'block', modes: BOTH, run: (h) => h.applyBlock('h2') },
	{ id: 'md.quote', icon: 'quote', title: 'editor.tool.quote', group: 'block', modes: BOTH, run: (h) => h.applyBlock('quote') },
	{ id: 'md.bullet', icon: 'list', title: 'editor.tool.bullet', group: 'block', modes: BOTH, run: (h) => h.applyBlock('bullet') },
	{ id: 'md.ordered', icon: 'list-ordered', title: 'editor.tool.ordered', group: 'block', modes: BOTH, run: (h) => h.applyBlock('ordered') },
	{ id: 'md.task', icon: 'list-todo', title: 'editor.tool.task', group: 'block', modes: BOTH, run: (h) => h.applyBlock('task') },
	{ id: 'md.bold', icon: 'bold', title: 'editor.tool.bold', group: 'inline', modes: BOTH, run: (h) => h.wrapInline('**', t('editor.tool.boldText')) },
	{ id: 'md.italic', icon: 'italic', title: 'editor.tool.italic', group: 'inline', modes: BOTH, run: (h) => h.wrapInline('*', t('editor.tool.italicText')) },
	{ id: 'md.strike', icon: 'strikethrough', title: 'editor.tool.strike', group: 'inline', modes: BOTH, run: (h) => h.wrapInline('~~', t('editor.tool.strikeText')) },
	{ id: 'md.code', icon: 'braces', title: 'editor.tool.code', group: 'inline', modes: BOTH, run: (h) => h.wrapInline('`', t('editor.tool.codeText')) },
	{ id: 'md.link', icon: 'link', title: 'editor.tool.link', group: 'inline', modes: BOTH, run: (h) => h.wrapAround('[', '](url)', t('editor.tool.linkText')) },
	{ id: 'md.table', icon: 'table', title: 'editor.tool.table', group: 'insert', modes: BOTH, run: (h) => h.insert(tableSnippet()) },
	{ id: 'md.hr', icon: 'minus', title: 'editor.tool.hr', group: 'insert', modes: BOTH, run: (h) => h.insert('\n---\n') },
	{ id: 'md.format', icon: 'sparkles', title: 'editor.tool.format', group: 'action', modes: BOTH, run: (h) => h.setValue(formatMarkdown(h.getValue())) },
]

builtins.forEach(registerEditorCommand)
