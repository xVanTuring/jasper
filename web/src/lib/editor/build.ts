// 编辑器编排器：创建 CodeMirror EditorView，装配基础扩展 + 变更/DOM 事件回调，
// 暴露一个控制器给 Svelte 组件。本文件（及其静态 import 的 @codemirror/*、livePreview、
// widgets 等）由组件在 onMount 里 `import()` 惰性加载 → 单独成 chunk，不进首屏包。

import { EditorView } from '@codemirror/view'
import {
	baseExtensions,
	modeCompartment,
	modeExtension,
	readOnlyCompartment,
	readOnlyExtension,
	pluginCompartment,
} from './setup'
import { makeHandle } from './handle'
import { editorExtensions, onEditorExtensionsChanged } from './editorExtensions'
import { refreshLivePreview } from './livePreview'
import { onResourceMetaChange } from '../resourceMeta.svelte'
import type { EditorHandle, EditorMode } from './types'

export interface EditorController {
	view: EditorView
	handle: EditorHandle
	setMode(m: EditorMode): void
	setReadOnly(ro: boolean): void
	destroy(): void
}

export interface CreateEditorOptions {
	parent: HTMLElement
	doc: string
	mode: EditorMode
	readOnly: boolean
	placeholderText?: string
	// value 变更回调；userEvent=真实用户输入（打字/删除/粘贴），供上层决定是否排编辑期插件变换
	onChange: (value: string, userEvent: boolean) => void
	// 粘贴/拖拽进来的文件（上传为资源）
	onFiles: (files: File[]) => void
}

// 从剪贴板/拖拽事件里取文件（含截图粘贴：文件在 items 里 kind==='file'）
function filesFrom(dt: DataTransfer | null): File[] {
	if (!dt) return []
	if (dt.files && dt.files.length) return Array.from(dt.files)
	const out: File[] = []
	for (const it of Array.from(dt.items || [])) {
		if (it.kind === 'file') {
			const f = it.getAsFile()
			if (f) out.push(f)
		}
	}
	return out
}

export function createEditor(opts: CreateEditorOptions): EditorController {
	let mode = opts.mode
	const view = new EditorView({
		parent: opts.parent,
		doc: opts.doc,
		extensions: [
			baseExtensions({ mode, readOnly: opts.readOnly, placeholderText: opts.placeholderText }),
			EditorView.updateListener.of((u) => {
				if (!u.docChanged) return
				const userEvent = u.transactions.some((tr) => tr.isUserEvent('input') || tr.isUserEvent('delete'))
				opts.onChange(u.state.doc.toString(), userEvent)
			}),
			EditorView.domEventHandlers({
				paste: (e) => {
					const f = filesFrom(e.clipboardData)
					if (!f.length) return false
					e.preventDefault()
					opts.onFiles(f)
					return true
				},
				drop: (e) => {
					const f = filesFrom(e.dataTransfer)
					if (!f.length) return false
					e.preventDefault()
					opts.onFiles(f)
					return true
				},
			}),
		],
	})

	const handle = makeHandle(view, () => mode)
	// 插件扩展注册表变化 → 热重构 compartment（无需重建实例）
	const unsubExt = onEditorExtensionsChanged(() =>
		view.dispatch({ effects: pluginCompartment.reconfigure(editorExtensions()) }),
	)
	// 资源元数据（id→mime）加载/更新 → 触发 Live Preview 重建，媒体 widget 据新 mime 重挑
	const unsubMeta = onResourceMetaChange(() => view.dispatch({ effects: refreshLivePreview.of(null) }))

	return {
		view,
		handle,
		setMode(m) {
			if (m === mode) return
			mode = m
			view.dispatch({ effects: modeCompartment.reconfigure(modeExtension(m)) })
		},
		setReadOnly(ro) {
			view.dispatch({ effects: readOnlyCompartment.reconfigure(readOnlyExtension(ro)) })
		},
		destroy() {
			unsubExt()
			unsubMeta()
			view.destroy()
		},
	}
}
