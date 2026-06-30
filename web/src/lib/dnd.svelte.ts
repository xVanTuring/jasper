// 跨组件共享的“正在拖拽的笔记本”状态。
// 用途：① FolderTree 放置时禁止把笔记本拖进自身或其后代（成环）——拖拽开始时算好
// forbidden 集合；② App 侧「移到顶层」放置区在拖拽笔记本时才显示。
let dragging = $state<{ id: string; forbidden: Set<string> } | null>(null)

export function startFolderDrag(id: string, forbidden: Set<string>) {
	dragging = { id, forbidden }
}
export function endFolderDrag() {
	dragging = null
}
/** 当前正在拖拽的笔记本（含其自身+后代 id 集合）；无则 null。 */
export function draggingFolder() {
	return dragging
}
