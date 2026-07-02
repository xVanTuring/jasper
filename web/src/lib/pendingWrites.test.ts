// pendingWrites.svelte.ts：写提案确认队列（spec 0.3 §9.5）。
import { describe, it, expect, vi } from 'vitest'
import type { PendingWrite } from './api'

function pw(over: Partial<PendingWrite> = {}): PendingWrite {
	return {
		action: 'update',
		plugin_id: 'p',
		note: { id: 'a'.repeat(32), parent_id: 'f'.repeat(32), title: 't', body: 'new' },
		original: { title: 't', body: 'old' },
		...over,
	}
}

async function freshStore() {
	vi.resetModules()
	return import('./pendingWrites.svelte')
}

describe('pendingWrites 队列', () => {
	it('入队/队首/出队/清空', async () => {
		const store = await freshStore()
		expect(store.currentPendingWrite()).toBeNull()

		store.enqueuePendingWrites([pw(), pw({ action: 'create', original: null })])
		expect(store.pendingWriteQueue().length).toBe(2)
		expect(store.currentPendingWrite()?.action).toBe('update')

		store.shiftPendingWrite()
		expect(store.currentPendingWrite()?.action).toBe('create')

		store.enqueuePendingWrites([]) // 空数组入队无副作用
		expect(store.pendingWriteQueue().length).toBe(1)

		store.clearPendingWrites()
		expect(store.currentPendingWrite()).toBeNull()
	})
})
