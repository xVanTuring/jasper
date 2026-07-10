// 编辑视图默认偏好（源码 / Live Preview 实时预览）：客户端本地（localStorage），
// 由 NoteView 与设置面板「编辑器」分区共享同一个 jasper.editor 键，避免字符串漂移。
// 统一到 CodeMirror 6 后取值为 'source' | 'live'；旧值 'wysiwyg'（Milkdown 时代）迁移为 'live'。
export const ENGINE_KEY = 'jasper.editor'
export type EditorEngine = 'source' | 'live'

/** 读默认视图；'source' → 'source'，其余（含未设置/旧值 'wysiwyg'）一律 'live'（默认实时预览）。 */
export function getEngine(): EditorEngine {
	try {
		return localStorage.getItem(ENGINE_KEY) === 'source' ? 'source' : 'live'
	} catch {
		return 'live'
	}
}

export function setEngine(e: EditorEngine): void {
	try {
		localStorage.setItem(ENGINE_KEY, e)
	} catch {
		/* localStorage 不可用 → 忽略 */
	}
}
