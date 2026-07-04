// chatSessions.ts：多会话模型 + localStorage 持久化（纯逻辑）。
import { describe, it, expect, beforeEach } from 'vitest'
import {
	loadStore,
	persistStore,
	emptyStore,
	appendMessage,
	startSession,
	switchSession,
	removeSession,
	renameSession,
	sessionTitle,
	activeSession,
	storageKeyFor,
	type SessionStore,
} from './chatSessions'

// 确定性 id 生成器
function counter() {
	let n = 0
	return () => `id${n++}`
}
const NOW = 1000

describe('chatSessions model', () => {
	beforeEach(() => localStorage.clear())

	it('缺失 → 单个空会话；持久化后可读回', () => {
		const gen = counter()
		const s = loadStore('k', gen, () => NOW)
		expect(s.sessions).toHaveLength(1)
		expect(s.activeId).toBe(s.sessions[0].id)

		const withMsg = appendMessage(s, { role: 'user', content: '你好' }, NOW + 1)
		persistStore('k', withMsg)
		const back = loadStore('k', counter(), () => NOW)
		expect(back.sessions[0].messages).toEqual([{ role: 'user', content: '你好' }])
		expect(back.activeId).toBe(withMsg.activeId)
	})

	it('appendMessage 追加到当前会话并冒泡到最前', () => {
		let store: SessionStore = emptyStore('a', 1)
		store = { sessions: [...store.sessions, { id: 'b', title: '', messages: [], updatedAt: 2 }], activeId: 'a' }
		store = appendMessage(store, { role: 'user', content: 'hi' }, 99)
		expect(activeSession(store).messages).toHaveLength(1)
		expect(store.sessions[0].id).toBe('a') // updatedAt=99 → 冒到最前
	})

	it('startSession：当前空会话则复用，否则新建切换', () => {
		const gen = counter()
		let store = emptyStore('a', 1)
		const same = startSession(store, gen, 5)
		expect(same.activeId).toBe('a') // 复用空会话
		store = appendMessage(store, { role: 'user', content: 'x' }, 6)
		const next = startSession(store, gen, 7)
		expect(next.activeId).not.toBe('a')
		expect(next.sessions).toHaveLength(2)
	})

	it('switch / rename / remove', () => {
		const gen = counter()
		let store: SessionStore = { sessions: [
			{ id: 'a', title: '', messages: [{ role: 'user', content: '第一条问题' }], updatedAt: 2 },
			{ id: 'b', title: '', messages: [], updatedAt: 1 },
		], activeId: 'a' }

		store = switchSession(store, 'b')
		expect(store.activeId).toBe('b')
		expect(switchSession(store, 'nope').activeId).toBe('b') // 未知 id 不动

		store = renameSession(store, 'a', '重命名了')
		expect(sessionTitle(store.sessions.find((s) => s.id === 'a')!)).toBe('重命名了')
		// 未命名会话标题从首条 user 消息派生
		expect(sessionTitle({ id: 'x', title: '', messages: [{ role: 'user', content: '首行\n第二行' }], updatedAt: 0 })).toBe('首行')

		// 删当前 → 切到剩下的
		store = removeSession(store, 'b', gen, 3)
		expect(store.sessions.map((s) => s.id)).toEqual(['a'])
		expect(store.activeId).toBe('a')
		// 删到空 → 自动补一个空会话
		store = removeSession(store, 'a', gen, 4)
		expect(store.sessions).toHaveLength(1)
		expect(store.sessions[0].messages).toHaveLength(0)
	})

	it('损坏的 localStorage → 退回新空会话', () => {
		localStorage.setItem('jasper.chat.k', '{ not json')
		const s = loadStore('k', counter(), () => NOW)
		expect(s.sessions).toHaveLength(1)
		expect(s.sessions[0].messages).toHaveLength(0)
	})

	it('storageKeyFor 组合 pluginId/panelId', () => {
		expect(storageKeyFor('ai-chat', 'chat-panel')).toBe('ai-chat/chat-panel')
	})
})
