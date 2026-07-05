// localStore（IndexedDB 持久层）单测：fake-indexeddb 提供内存态 IDB。
import 'fake-indexeddb/auto'
import { beforeEach, describe, expect, it } from 'vitest'
import * as localStore from './localStore'

// 每个用例前：先关连接（否则 deleteDatabase 被阻塞），再清库，保证隔离。
beforeEach(async () => {
	await localStore._resetForTest()
	await new Promise<void>((resolve) => {
		const req = indexedDB.deleteDatabase('jasper-local')
		req.onsuccess = () => resolve()
		req.onerror = () => resolve()
		req.onblocked = () => resolve()
	})
})

describe('localStore raws', () => {
	it('首次运行 loadRaws 返回 null', async () => {
		expect(await localStore.loadRaws()).toBeNull()
	})

	it('saveRaws → loadRaws 往返', async () => {
		const raws = ['a\n\nid: x\ntype_: 1', 'b\n\nid: y\ntype_: 2']
		await localStore.saveRaws(raws)
		expect(await localStore.loadRaws()).toEqual(raws)
	})

	it('saveRaws 覆盖（后写为准，含删除）', async () => {
		await localStore.saveRaws(['one'])
		await localStore.saveRaws(['two', 'three'])
		expect(await localStore.loadRaws()).toEqual(['two', 'three'])
	})

	it('空数组也能持久（区别于「从未写过」的 null）', async () => {
		await localStore.saveRaws([])
		expect(await localStore.loadRaws()).toEqual([])
	})
})

describe('localStore seeded', () => {
	it('默认未播种，markSeeded 后为真', async () => {
		expect(await localStore.isSeeded()).toBe(false)
		await localStore.markSeeded()
		expect(await localStore.isSeeded()).toBe(true)
	})
})

describe('localStore resources', () => {
	it('put → get 往返记录（id + mime）', async () => {
		// 注：jsdom/fake-indexeddb 下 jsdom 的 Blob 经 structuredClone 会退化为空对象，
		// 故此处只断言记录结构（id/mime）往返；二进制完整性由真实浏览器 IDB 保证（原样存 Blob），
		// 在 build:local 的实机验证里覆盖。
		const blob = new Blob([new Uint8Array([1, 2, 3])], { type: 'image/png' })
		await localStore.putResource('r1', 'image/png', blob)
		const got = await localStore.getResource('r1')
		expect(got?.id).toBe('r1')
		expect(got?.mime).toBe('image/png')
	})

	it('allResources 汇总，delete 移除', async () => {
		await localStore.putResource('r1', 'image/png', new Blob(['x']))
		await localStore.putResource('r2', 'text/plain', new Blob(['y']))
		expect((await localStore.allResources()).map((r) => r.id).sort()).toEqual(['r1', 'r2'])
		await localStore.deleteResource('r1')
		expect((await localStore.allResources()).map((r) => r.id)).toEqual(['r2'])
		expect(await localStore.getResource('r1')).toBeUndefined()
	})
})
