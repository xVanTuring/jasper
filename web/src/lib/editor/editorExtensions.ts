// 编辑器扩展贡献点（客户端）。这是「编辑器可被扩展」的内部接缝：
// 任何模块（内置功能、将来的客户端插件）都能往这里注册 CodeMirror 扩展，
// 编辑器组件会把它们叠加进一个 compartment，并在注册表变化时热重构。
//
// 说明：Jasper 现有插件是服务端 WASM + 声明式 manifest，尚无「向前端注入 JS」的通道，
// 故第三方插件目前仍通过既有服务端钩子扩展编辑（editor.transform 文本变换、note-toolbar
// 命令，二者在统一到 CM6 后照常生效）。本注册表先把接缝定好、供内置功能模块化使用，
// 将来若开放客户端插件通道即可直接接入，无需再动编辑器核心。

import type { Extension } from '@codemirror/state'

export interface EditorExtensionEntry {
	/** 稳定 id：同 id 再注册即替换（便于热替换/更新）。 */
	id: string
	extension: Extension
}

const registry: EditorExtensionEntry[] = []
const listeners = new Set<() => void>()

function notify() {
	for (const l of listeners) l()
}

/** 注册（或按 id 替换）一个编辑器扩展；返回注销函数。 */
export function registerEditorExtension(entry: EditorExtensionEntry): () => void {
	const i = registry.findIndex((e) => e.id === entry.id)
	if (i >= 0) registry[i] = entry
	else registry.push(entry)
	notify()
	return () => {
		const j = registry.findIndex((e) => e.id === entry.id)
		if (j >= 0) {
			registry.splice(j, 1)
			notify()
		}
	}
}

/** 当前已注册的扩展列表（按注册顺序）。 */
export function editorExtensions(): Extension[] {
	return registry.map((e) => e.extension)
}

/** 订阅注册表变化（编辑器组件据此重构 compartment）；返回取消订阅。 */
export function onEditorExtensionsChanged(cb: () => void): () => void {
	listeners.add(cb)
	return () => listeners.delete(cb)
}
