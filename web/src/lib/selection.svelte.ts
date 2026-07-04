// 笔记内容选区捕获（供插件侧栏把「当前选中的文字」并入命令 args，spec §9.2）。
//
// 全局监听 document 的 selectionchange：
// - 选区落在标了 `data-ai-selectable` 的容器内（阅读视图 / 编辑器）→ 记住其文字（空则清空）。
// - 选区落在容器外（如聊天输入框、侧栏）→ **保持不变**：点进聊天发送时不该丢掉刚在笔记里选的文字。
// 打开的笔记切换时由 App 调 clearSelection() 兜底，避免跨笔记的陈旧选区。

let selectedText = $state('')

function withinSelectable(node: Node | null): boolean {
	const el = node instanceof Element ? node : (node?.parentElement ?? null)
	return !!el?.closest('[data-ai-selectable]')
}

let installed = false

/** 安装一次全局 selectionchange 捕获（App 启动时调）。SSR/无 document 时空转。 */
export function installSelectionCapture(): void {
	if (installed || typeof document === 'undefined') return
	installed = true
	document.addEventListener('selectionchange', () => {
		const sel = document.getSelection()
		if (!sel || sel.rangeCount === 0) return
		// 只在选区锚点位于笔记内容区时更新；否则（聊天/其它 UI）保持已存选区不动
		if (!withinSelectable(sel.anchorNode)) return
		selectedText = sel.toString().trim()
	})
}

/** 当前笔记内容区里选中的文字（无选区 = 空串）。响应式读取（rune）。 */
export function currentSelectionText(): string {
	return selectedText
}

/** 清空已存选区（切换笔记时兜底）。 */
export function clearSelection(): void {
	selectedText = ''
}
