// 插件写提案确认队列（spec 0.3 §9.5）：command/ui 响应里的 pending_writes 入队，
// PendingWriteDialog 逐条弹 diff 确认——同意走普通 PUT/POST /api/notes*，拒绝出队即弃。

import type { PendingWrite } from './api'

let queue = $state<PendingWrite[]>([])

export function pendingWriteQueue(): PendingWrite[] {
	return queue
}

/** 队首（当前待确认项）；空队列返回 null。 */
export function currentPendingWrite(): PendingWrite | null {
	return queue[0] ?? null
}

export function enqueuePendingWrites(writes: PendingWrite[]): void {
	if (writes.length) queue = [...queue, ...writes]
}

/** 出队队首（同意已应用 / 拒绝丢弃后调用）。 */
export function shiftPendingWrite(): void {
	queue = queue.slice(1)
}

export function clearPendingWrites(): void {
	queue = []
}
