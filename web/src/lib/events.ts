// SSE 变更流客户端（GET /api/events）。服务端在一切写路径上广播 (kind, op, id)，
// 内容由调用方按需再拉——见 App.svelte 的去抖合并刷新。
// 断线由 EventSource 自动重连；重连间隙可能漏事件，故重连成功时合成一条
// library reload 交给调用方全量刷新兜底（服务端重启/网络抖动都被覆盖）。
import { IS_WASM } from './api'

export type ChangeEvent = {
	kind: 'note' | 'folder' | 'tag' | 'library'
	op: 'upsert' | 'delete' | 'reload'
	id: string // kind=tag 时为受影响的笔记 id
}

let source: EventSource | null = null

/** 建立事件订阅（幂等：已连接/无后端 WASM 构建/环境不支持 → false）。 */
export function connectEvents(onChange: (ev: ChangeEvent) => void): boolean {
	if (IS_WASM || source || typeof EventSource === 'undefined') return false
	const es = new EventSource('/api/events')
	source = es
	let openedOnce = false
	es.onopen = () => {
		// 重连成功（非首连）：间隙内的事件已丢 → 全量刷新
		if (openedOnce) onChange({ kind: 'library', op: 'reload', id: '' })
		openedOnce = true
	}
	es.addEventListener('change', (e) => {
		try {
			onChange(JSON.parse((e as MessageEvent).data) as ChangeEvent)
		} catch {
			/* 坏帧忽略 */
		}
	})
	// onerror 不关闭：EventSource 自带指数退避重连
	return true
}

/** 断开（测试/登出用）。 */
export function disconnectEvents() {
	source?.close()
	source = null
}
