// 笔记本树的展开/折叠记忆：客户端本地（localStorage），与「记忆上次打开的笔记」
// (jasper.lastNote) / 「编辑器引擎」(jasper.editor) 同姿势——只存**展开**的笔记本 id 集合，
// 重载时恢复（未记录的一律折叠，与旧「默认折叠」行为一致，只是这次会被记住）。
//
// FolderTree 是递归组件，若各实例各存一份 expanded 就无法整棵树共享、也无处持久。
// 故把状态抽到本模块的单一 rune 集合：各层实例通过 isExpanded()/toggleExpanded() 读写同一份，
// 模板里读 isExpanded() 会建立对该 rune 的依赖，切换即时重渲染。

import { SvelteSet } from 'svelte/reactivity'

const KEY = 'jasper.expandedFolders'

function load(): string[] {
	try {
		const raw = localStorage.getItem(KEY)
		if (!raw) return []
		const arr: unknown = JSON.parse(raw)
		return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === 'string') : []
	} catch {
		return []
	}
}

// 展开的笔记本 id 集合。必须用 svelte/reactivity 的 SvelteSet：
// 原生 Set 经 $state() 并不会让 add/delete/has 变响应式（Svelte 5 只深代理普通对象/数组，
// 不代理 Set/Map），会导致 caret 点击后 has() 读取不重算、树不重渲染（折叠失效）。
// SvelteSet 的 add/delete/has 都被追踪；只原地增删、绝不整体重赋值，跨模块共享同一实例。
const expanded = new SvelteSet<string>(load())

function persist(): void {
	try {
		localStorage.setItem(KEY, JSON.stringify([...expanded]))
	} catch {
		/* localStorage 不可用（隐私模式/被禁用）→ 仅本次会话生效 */
	}
}

/** 该笔记本当前是否展开。 */
export function isExpanded(id: string): boolean {
	return expanded.has(id)
}

/** 设定展开态并持久化。 */
export function setExpanded(id: string, open: boolean): void {
	if (open) expanded.add(id)
	else expanded.delete(id)
	persist()
}

/** 切换展开态并持久化（供 caret 点击调用）。 */
export function toggleExpanded(id: string): void {
	setExpanded(id, !expanded.has(id))
}

/** 测试辅助：清空内存态与持久值，隔离用例。 */
export function _resetForTest(): void {
	expanded.clear()
	try {
		localStorage.removeItem(KEY)
	} catch {
		/* ignore */
	}
}
