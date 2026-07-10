// Live Preview 的块级/替换 widget：把一段 markdown 源码「成型」为可视元素，
// 光标进入其范围时 livePreview 会撤下替换、露出原始源码可编辑（见 livePreview.ts）。
// 图片解析 :/id、表格复用阅读视图的 renderMarkdown、公式用 KaTeX、待办勾选框可点击回写。

import { WidgetType, type EditorView } from '@codemirror/view'
import katex from 'katex'
import { renderMarkdown } from '../render'
import { parseResourceId, api } from '../api'
import { renderPdfInto, type PdfHandle } from '../pdfRender'
import 'katex/dist/katex.min.css'

// —— 图片：![alt](url)，:/id 经 api.resourceUrl 解析为真实地址 ——
export class ImageWidget extends WidgetType {
	constructor(
		readonly url: string,
		readonly alt: string,
	) {
		super()
	}
	eq(other: ImageWidget) {
		return other.url === this.url && other.alt === this.alt
	}
	toDOM() {
		const wrap = document.createElement('span')
		wrap.className = 'cm-lp-image'
		const img = document.createElement('img')
		const id = parseResourceId(this.url)
		img.src = id ? api.resourceUrl(id) : this.url
		if (this.alt) img.alt = this.alt
		img.loading = 'lazy'
		wrap.appendChild(img)
		if (this.alt) {
			const cap = document.createElement('span')
			cap.className = 'cm-lp-image-cap'
			cap.textContent = this.alt
			wrap.appendChild(cap)
		}
		return wrap
	}
	// 让点击落到编辑器（把光标定到 widget 边界）→ 该处源码露出可编辑
	ignoreEvent() {
		return false
	}
}

// —— 视频 / 音频：![](:/id) 指向 video/* 或 audio/* 资源时，成型为可播放的媒体元素 ——
export class MediaWidget extends WidgetType {
	constructor(
		readonly url: string,
		readonly kind: 'video' | 'audio',
		readonly alt: string,
	) {
		super()
	}
	eq(other: MediaWidget) {
		return other.url === this.url && other.kind === this.kind && other.alt === this.alt
	}
	toDOM() {
		const el = document.createElement(this.kind) as HTMLMediaElement
		const id = parseResourceId(this.url)
		el.src = id ? api.resourceUrl(id) : this.url
		el.controls = true
		el.preload = 'metadata'
		el.className = this.kind === 'video' ? 'cm-lp-video' : 'cm-lp-audio'
		if (this.alt) el.title = this.alt
		return el
	}
	ignoreEvent() {
		return true // 事件留给媒体控件（播放/进度），不移动编辑器光标
	}
}

// —— PDF 内联嵌入：正文里直接渲染 PDF（Obsidian 式），工具栏「全屏」派发 jasper-open-pdf 打开模态 ——
export class PdfEmbedWidget extends WidgetType {
	private handle?: PdfHandle
	constructor(
		readonly id: string,
		readonly name: string,
	) {
		super()
	}
	eq(other: PdfEmbedWidget) {
		// id+name 稳定 → CM 复用同一 DOM，不因每次装饰重建而重载 pdf.js
		return other.id === this.id && other.name === this.name
	}
	toDOM() {
		const wrap = document.createElement('div')
		wrap.className = 'cm-lp-pdf'
		this.handle = renderPdfInto(wrap, {
			url: api.resourceUrl(this.id),
			name: this.name,
			id: this.id,
			onExpand: () =>
				wrap.dispatchEvent(new CustomEvent('jasper-open-pdf', { bubbles: true, detail: { id: this.id, name: this.name } })),
		})
		return wrap
	}
	destroy() {
		this.handle?.destroy()
	}
	ignoreEvent() {
		return true // 事件留给内联阅读器工具栏（翻页/缩放/全屏），不移动编辑器光标
	}
}

// —— 文件卡片：非媒体资源（zip/doc…）成型为下载链接 ——
export class FileCardWidget extends WidgetType {
	constructor(
		readonly url: string,
		readonly name: string,
	) {
		super()
	}
	eq(other: FileCardWidget) {
		return other.url === this.url && other.name === this.name
	}
	toDOM() {
		const a = document.createElement('a')
		const id = parseResourceId(this.url)
		a.href = id ? api.resourceUrl(id) : this.url
		a.download = this.name
		a.className = 'file-card'
		a.textContent = this.name
		return a
	}
	ignoreEvent() {
		return true
	}
}

// —— 无序列表项目符号：把 `-`/`*`/`+` 显示为 • 圆点（光标在该行时露出原字符）——
export class BulletWidget extends WidgetType {
	eq() {
		return true
	}
	toDOM() {
		const el = document.createElement('span')
		el.className = 'cm-lp-bullet'
		el.textContent = '•'
		return el
	}
	ignoreEvent() {
		return false
	}
}

// —— 分隔线 --- / *** / ___ ——
export class HrWidget extends WidgetType {
	eq() {
		return true
	}
	toDOM() {
		const el = document.createElement('span')
		el.className = 'cm-lp-hr'
		el.appendChild(document.createElement('hr'))
		return el
	}
	get estimatedHeight() {
		return 20
	}
	ignoreEvent() {
		return false
	}
}

// —— 表格：整块渲染为 HTML 表（复用阅读视图管线，已净化）——
export class TableWidget extends WidgetType {
	constructor(readonly source: string) {
		super()
	}
	eq(other: TableWidget) {
		return other.source === this.source
	}
	toDOM() {
		const el = document.createElement('div')
		el.className = 'cm-lp-table'
		el.innerHTML = renderMarkdown(this.source)
		return el
	}
	ignoreEvent() {
		return false
	}
}

// —— 数学公式：$…$（行内）/ $$…$$（块级），KaTeX 渲染 ——
export class MathWidget extends WidgetType {
	constructor(
		readonly tex: string,
		readonly display: boolean,
	) {
		super()
	}
	eq(other: MathWidget) {
		return other.tex === this.tex && other.display === this.display
	}
	toDOM() {
		const el = document.createElement(this.display ? 'div' : 'span')
		el.className = this.display ? 'cm-lp-math-block' : 'cm-lp-math'
		try {
			el.innerHTML = katex.renderToString(this.tex, {
				throwOnError: false,
				displayMode: this.display,
			})
		} catch {
			el.textContent = this.tex
		}
		return el
	}
	// 点击公式即把光标定到边界 → 露出 $…$ 源码可编辑（否则 widget 吞掉事件无法进入）
	ignoreEvent() {
		return false
	}
}

// —— 待办勾选框：替换 `[ ]` / `[x]` 标记，点击即回写文档 ——
export class CheckboxWidget extends WidgetType {
	constructor(
		readonly checked: boolean,
		readonly pos: number,
	) {
		super()
	}
	eq(other: CheckboxWidget) {
		return other.checked === this.checked && other.pos === this.pos
	}
	toDOM(view: EditorView) {
		const box = document.createElement('input')
		box.type = 'checkbox'
		box.className = 'cm-lp-task'
		box.checked = this.checked
		box.addEventListener('mousedown', (e) => e.preventDefault())
		box.addEventListener('change', () => {
			// TaskMarker 为 `[ ]`/`[x]` 三字符；中间字符切换 空格 ↔ x
			view.dispatch({
				changes: { from: this.pos + 1, to: this.pos + 2, insert: this.checked ? ' ' : 'x' },
			})
			view.focus()
		})
		return box
	}
	ignoreEvent() {
		return false // 让 change/mousedown 事件到达 widget 自身处理
	}
}
