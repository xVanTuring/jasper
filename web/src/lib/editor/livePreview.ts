// Live Preview（实时预览）装饰层 —— Obsidian 同款原理：
// 遍历 @codemirror/lang-markdown 的 Lezer 语法树，为文档构建装饰：
//   · 行内格式（粗/斜/删除线/行内代码/链接）就地成型并隐藏语法标记；
//   · 块级元素（图片/表格/分隔线/待办勾选框）替换为 widget；
//   · 光标/选区进入某节点范围时撤下该处装饰、露出原始源码可编辑（reveal-on-cursor）。
// 数学公式 $…$/$$…$$ 与 ==高亮== 不在 GFM 语法树里，用正则补扫。
//
// 用 StateField 而非 ViewPlugin：CM 规定「跨行 / block 的 replace 装饰」（表格、多行公式）
// 只能由 StateField 提供，ViewPlugin 会抛错。笔记体量小，全文构建（不做视口裁剪）即可，
// StateField 随文档/选区变化重建，reveal-on-cursor 照常工作。

import { Decoration, WidgetType, EditorView, type DecorationSet } from '@codemirror/view'
import { syntaxTree } from '@codemirror/language'
import { StateField, StateEffect, type Range, type EditorState } from '@codemirror/state'
import type { SyntaxNode } from '@lezer/common'
import { ImageWidget, HrWidget, TableWidget, MathWidget, CheckboxWidget, BulletWidget, MediaWidget, PdfEmbedWidget, FileCardWidget } from './widgets'
import { parseResourceId } from '../api'
import { mediaKind, resourceTitle } from '../resourceMeta.svelte'

const hidden = Decoration.replace({})

interface Built {
	deco: DecorationSet
	// 仅 widget 替换范围设为原子（光标跳过、方向键跨越）；mark/隐藏语法标记不入此集，
	// 否则粗体等会变得无法进入编辑、隐藏标记也无法露出。
	atomic: DecorationSet
}

// 某祖先是否为代码上下文（数学/高亮补扫时用来排除代码内的 $、== 等）
function inCode(node: SyntaxNode | null): boolean {
	for (let n = node; n; n = n.parent) {
		if (n.name === 'FencedCode' || n.name === 'CodeBlock' || n.name === 'InlineCode') return true
	}
	return false
}

function build(state: EditorState): Built {
	const doc = state.doc
	const sel = state.selection.ranges
	// 选区（含光标）是否与 [from,to] 相交 → 相交则露出源码（用于行内格式的逐节点隐现）
	const active = (from: number, to: number) => sel.some((r) => r.from <= to && r.to >= from)
	const lineActive = (pos: number) => {
		const l = doc.lineAt(pos)
		return active(l.from, l.to)
	}
	// 行级露出：光标落在 [from,to] 所跨任意一行 → 露出源码。用于块级/原子 widget
	//（图片/表格/公式/分隔线），点击该行即可进入编辑，避免被原子 widget 卡住无法编辑。
	const reveal = (from: number, to: number) => active(doc.lineAt(from).from, doc.lineAt(to).to)

	const out: Range<Decoration>[] = []
	const atomic: Range<Decoration>[] = []
	const hide = (from: number, to: number) => {
		if (to > from) out.push(hidden.range(from, to))
	}
	const mark = (from: number, to: number, cls: string) => {
		if (to > from) out.push(Decoration.mark({ class: cls }).range(from, to))
	}
	const lineDeco = (pos: number, cls: string) => out.push(Decoration.line({ class: cls }).range(doc.lineAt(pos).from))
	const eachLine = (from: number, to: number, cls: string) => {
		const a = doc.lineAt(from).number
		const b = doc.lineAt(to).number
		for (let n = a; n <= b; n++) lineDeco(doc.line(n).from, cls)
	}
	const replaceWith = (from: number, to: number, widget: WidgetType) => {
		const d = Decoration.replace({ widget }).range(from, to)
		out.push(d)
		atomic.push(d)
	}
	const consumed: [number, number][] = [] // 已被 widget 占用（数学/高亮补扫时避让）

	syntaxTree(state).iterate({
		enter: (node) => {
			const name = node.name
			const nf = node.from
			const nt = node.to
			switch (name) {
				case 'ATXHeading1':
				case 'ATXHeading2':
				case 'ATXHeading3':
				case 'ATXHeading4':
				case 'ATXHeading5':
				case 'ATXHeading6':
					lineDeco(nf, `cm-hd cm-hd-${name.slice(-1)}`)
					return
				case 'SetextHeading1':
				case 'SetextHeading2':
					lineDeco(nf, `cm-hd cm-hd-${name.endsWith('1') ? 1 : 2}`)
					return
				case 'HeaderMark': {
					// `#`（及尾随空格）/ setext 下划线：所在行非活动时隐藏
					if (!lineActive(nf)) {
						let end = nt
						if (doc.sliceString(nt, nt + 1) === ' ') end = nt + 1
						hide(nf, end)
					}
					return
				}
				case 'Blockquote':
					eachLine(nf, nt, 'cm-blockquote')
					return
				case 'QuoteMark': {
					if (!lineActive(nf)) {
						let end = nt
						if (doc.sliceString(nt, nt + 1) === ' ') end = nt + 1
						hide(nf, end)
					}
					return
				}
				case 'ListMark': {
					if (!lineActive(nf)) {
						const line = doc.lineAt(nf)
						if (/^\s*\[[ xX]\]/.test(doc.sliceString(nt, line.to))) {
							// 任务项（`- [ ]`/`* [x]`）：勾选框即标记，隐藏前面的 `-`/`*`（含尾随空格）
							let end = nt
							if (doc.sliceString(nt, nt + 1) === ' ') end = nt + 1
							hide(nf, end)
						} else if (/^[-*+]$/.test(doc.sliceString(nf, nt))) {
							// 无序列表：`-`/`*`/`+` → • 圆点
							replaceWith(nf, nt, new BulletWidget())
						}
					}
					return
				}
				case 'StrongEmphasis':
					mark(nf, nt, 'cm-strong')
					return
				case 'Emphasis':
					mark(nf, nt, 'cm-em')
					return
				case 'Strikethrough':
					// GFM 删除线的 `~~` 定界符不是独立的 mark 节点 → 显式隐藏两侧各 2 字符
					mark(nf, nt, 'cm-strike')
					if (!active(nf, nt)) {
						hide(nf, nf + 2)
						hide(nt - 2, nt)
					}
					return
				case 'InlineCode':
					mark(nf, nt, 'cm-inline-code')
					return
				case 'EmphasisMark':
				case 'CodeMark': {
					const p = node.node.parent
					if (p && !active(p.from, p.to)) hide(nf, nt)
					return
				}
				case 'Link': {
					// Joplin 用链接语法 `[名字](:/id)` 嵌入非图片资源 → 视频/音频/PDF 也要成型
					const raw = doc.sliceString(nf, nt)
					const lm = /^\[([^\]]*)\]\(\s*([^)\s]+)/.exec(raw)
					const lid = lm ? parseResourceId(lm[2]) : null
					const lkind = lid ? mediaKind(lid) : 'unknown'
					if (lm && lid && !reveal(nf, nt) && (lkind === 'video' || lkind === 'audio' || lkind === 'pdf')) {
						const nm = lm[1] || resourceTitle(lid) || 'file'
						const w = lkind === 'pdf' ? new PdfEmbedWidget(lid, nm) : new MediaWidget(lm[2], lkind, nm)
						replaceWith(nf, nt, w)
						consumed.push([nf, nt])
						return false
					}
					mark(nf, nt, 'cm-link')
					return
				}
				case 'LinkMark': {
					const p = node.node.parent
					if (p && !active(p.from, p.to)) hide(nf, nt)
					return
				}
				case 'URL':
				case 'LinkTitle': {
					const p = node.node.parent
					if (p && p.name === 'Link' && !active(p.from, p.to)) hide(nf, nt)
					return
				}
				case 'Image': {
					if (!reveal(nf, nt)) {
						const raw = doc.sliceString(nf, nt)
						const m = /!\[([^\]]*)\]\(\s*([^)\s]+)(?:\s+"[^"]*")?\s*\)/.exec(raw)
						if (m) {
							const url = m[2]
							const alt = m[1]
							const id = parseResourceId(url)
							const kind = id ? mediaKind(id) : 'image'
							const name = alt || (id ? resourceTitle(id) : '') || 'file'
							const w =
								kind === 'video' || kind === 'audio'
									? new MediaWidget(url, kind, alt)
									: kind === 'pdf' && id
										? new PdfEmbedWidget(id, name)
										: kind === 'file'
											? new FileCardWidget(url, name)
											: new ImageWidget(url, alt)
							replaceWith(nf, nt, w)
							consumed.push([nf, nt])
							return false
						}
					}
					return
				}
				case 'HorizontalRule':
					if (!reveal(nf, nt)) replaceWith(nf, nt, new HrWidget())
					return
				case 'Table':
					if (!reveal(nf, nt)) {
						replaceWith(nf, nt, new TableWidget(doc.sliceString(nf, nt)))
						consumed.push([nf, nt])
						return false
					}
					return
				case 'FencedCode':
				case 'CodeBlock':
					eachLine(nf, nt, 'cm-code-block')
					consumed.push([nf, nt])
					return
				case 'CodeInfo':
					mark(nf, nt, 'cm-fence-info')
					return
				case 'TaskMarker': {
					if (!lineActive(nf)) {
						const checked = /x/i.test(doc.sliceString(nf, nt))
						replaceWith(nf, nt, new CheckboxWidget(checked, nf))
						return false
					}
					return
				}
			}
		},
	})

	// —— 正则补扫：数学 / 高亮（GFM 语法树没有）——
	const tree = syntaxTree(state)
	const text = doc.toString()
	const local: [number, number][] = []
	const isConsumed = (s: number, e: number) => consumed.some(([a, b]) => s < b && e > a)
	const free = (s: number, e: number) =>
		!reveal(s, e) && !isConsumed(s, e) && !local.some(([a, b]) => s < b && e > a) && !inCode(tree.resolveInner(s, 1))
	const run = (re: RegExp, fn: (m: RegExpExecArray, s: number, e: number) => void) => {
		re.lastIndex = 0
		let m: RegExpExecArray | null
		while ((m = re.exec(text))) {
			const s = m.index
			const e = s + m[0].length
			if (!free(s, e)) continue
			local.push([s, e])
			fn(m, s, e)
		}
	}
	// 块级公式 $$…$$（可跨行）优先，避免行内规则切进块内
	run(/\$\$([\s\S]+?)\$\$/g, (m, s, e) => replaceWith(s, e, new MathWidget(m[1].trim(), true)))
	run(/(?<![\\$])\$(?!\s)([^\n$]+?)(?<!\s)\$(?!\$)/g, (m, s, e) => replaceWith(s, e, new MathWidget(m[1], false)))
	run(/==(?!\s)([^\n=]+?)(?<!\s)==/g, (_m, s, e) => {
		hide(s, s + 2)
		mark(s + 2, e - 2, 'cm-highlight')
		hide(e - 2, e)
	})

	return { deco: Decoration.set(out, true), atomic: Decoration.set(atomic, true) }
}

// 外部触发重建（如资源元数据 id→mime 加载完成后，媒体 widget 需据新 mime 重挑）。
export const refreshLivePreview = StateEffect.define<null>()

/** Live Preview 扩展：光标离开时把 markdown 语法就地成型，进入时露出源码。 */
export const livePreview = StateField.define<Built>({
	create: (state) => build(state),
	update: (value, tr) =>
		tr.docChanged || tr.selection || tr.effects.some((e) => e.is(refreshLivePreview)) ? build(tr.state) : value,
	provide: (f) => [
		EditorView.decorations.from(f, (v) => v.deco),
		// 仅 widget 替换范围原子化：方向键跨越、点击落到边界而非内部
		EditorView.atomicRanges.of((view) => view.state.field(f, false)?.atomic ?? Decoration.none),
	],
})
