// CodeMirror 6 主题：全部走 app 调色板 CSS 变量（--bg/--text/--accent…），
// 故明暗切换由 CSS 变量自动完成，无需重建编辑器或切换 compartment。
// 同一套主题供源码模式与 Live Preview 复用，观感统一。

import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import type { Extension } from '@codemirror/state'

// 编辑区基础外观。选择器尽量贴合 CM 默认结构；行内 Live Preview 的语义类
// （cm-strong/cm-em/…）由本主题一并给出，两模式共享。
const base = EditorView.theme({
	'&': {
		color: 'var(--text)',
		backgroundColor: 'transparent',
		height: '100%',
		fontSize: '15px',
	},
	'.cm-scroller': {
		fontFamily: 'inherit',
		lineHeight: '1.7',
		overflow: 'auto',
	},
	'.cm-content': {
		// 水平留白由 .cm-content 自带（与标题 36px 对齐）；滚动条在外层 .cm-scroller 上贴右缘。
		padding: '6px 36px 40vh',
		caretColor: 'var(--accent)',
		// 内容宽度（铺满 / 居中限宽）由 NoteView 的 .width-* 类控制，见 NoteView.svelte
	},
	'&.cm-focused': { outline: 'none' },
	'.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--accent)' },
	'&.cm-focused .cm-cursor': { borderLeftColor: 'var(--accent)' },
	// 选区色：半透明强调色，明暗两主题都保证与正文对比（真正生效的强制覆盖在组件全局 CSS，
	// 因 CM 的 drawSelection 基础主题带 &light/&dark 前缀、特异性更高，见 CodeMirrorEditor.svelte）
	'.cm-selectionBackground, ::selection': { background: 'color-mix(in srgb, var(--accent) 30%, transparent)' },
	'&.cm-focused .cm-selectionBackground, &.cm-focused ::selection': {
		background: 'color-mix(in srgb, var(--accent) 30%, transparent)',
	},
	'.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--hover) 45%, transparent)' },
	'.cm-matchingBracket, &.cm-focused .cm-matchingBracket': {
		backgroundColor: 'var(--hover)',
		outline: '1px solid var(--border)',
	},
	// —— 代码等宽字体统一口径 ——
	'.cm-inline-code, .cm-code-block, .cm-fence-info': {
		fontFamily: "'SF Mono', Menlo, Consolas, monospace",
	},
})

// 语法高亮：源码模式主要靠它上色；Live Preview 里成型的语义靠 CSS 类，
// 但保留同一套 token 颜色让露出的原始语法（光标所在行）也和谐。
const highlight = HighlightStyle.define([
	{ tag: [t.heading, t.heading1, t.heading2, t.heading3, t.heading4, t.heading5, t.heading6], color: 'var(--text)', fontWeight: '700' },
	{ tag: t.strong, fontWeight: '700' },
	{ tag: t.emphasis, fontStyle: 'italic' },
	{ tag: t.strikethrough, textDecoration: 'line-through' },
	{ tag: [t.link, t.url], color: 'var(--accent)' },
	{ tag: t.monospace, fontFamily: "'SF Mono', Menlo, Consolas, monospace", color: 'var(--text)' },
	{ tag: t.quote, color: 'var(--text-dim)' },
	// 列表标记不上强调色（否则 t.list 会把列表项正文也染成紫色）；分隔线用弱色
	{ tag: t.contentSeparator, color: 'var(--text-dim)' },
	// 语法标记本身（#、**、` 等）在露出时用弱色，减轻视觉噪音
	{ tag: [t.processingInstruction, t.meta], color: 'var(--text-dim)' },
	{ tag: t.comment, color: 'var(--text-dim)', fontStyle: 'italic' },
	{ tag: t.invalid, color: 'var(--danger)' },
	// —— 围栏代码块内的语法着色（配合 markdown codeLanguages 嵌套语言）——
	// 用一套明暗都够对比的中性色，避免依赖 data-theme（自定义主题也适用）。注释复用上面的弱色。
	{ tag: [t.keyword, t.moduleKeyword, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], color: '#b054c8' },
	{ tag: [t.string, t.special(t.string), t.regexp], color: '#3f9a4f' },
	{ tag: [t.number, t.bool, t.null, t.atom], color: '#bd7b1e' },
	{ tag: [t.function(t.variableName), t.function(t.propertyName), t.labelName], color: '#4b83e0' },
	{ tag: [t.typeName, t.className, t.namespace, t.tagName], color: '#2a9d9d' },
	{ tag: [t.propertyName, t.attributeName], color: '#4b83e0' },
	{ tag: [t.operator, t.punctuation, t.bracket, t.derefOperator, t.separator], color: 'var(--text-dim)' },
	{ tag: t.escape, color: '#bd7b1e' },
])

/** 主题扩展集：基础外观 + 语法高亮。明暗随 CSS 变量自动切换。 */
export const editorThemeExtensions: Extension = [base, syntaxHighlighting(highlight)]
