// 编辑引擎默认偏好（源码 / 富文本）：客户端本地（localStorage），
// 由 NoteView 与设置面板「编辑器」分区共享同一个 jasper.editor 键，避免字符串漂移。
export const ENGINE_KEY = 'jasper.editor'
export type EditorEngine = 'source' | 'wysiwyg'

/** 读默认引擎；非 'wysiwyg' 一律回落 'source'（无损兜底）。 */
export function getEngine(): EditorEngine {
	try {
		return localStorage.getItem(ENGINE_KEY) === 'wysiwyg' ? 'wysiwyg' : 'source'
	} catch {
		return 'source'
	}
}

export function setEngine(e: EditorEngine): void {
	try {
		localStorage.setItem(ENGINE_KEY, e)
	} catch {
		/* localStorage 不可用 → 忽略 */
	}
}
