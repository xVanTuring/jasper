// 聊天会话模型：多会话 + localStorage 持久化（纯逻辑，可单测）。
// ChatWidget 持有 $state<SessionStore> + $effect 持久化；这里只放不可变的模型与增删改。
// 按 storageKey（pluginId/panelId）隔离，故不同插件面板各自成组、互不干扰。
import type { ChatMessage } from './api'

export interface ChatSession {
	id: string
	title: string // 显式命名；空则由 sessionTitle() 从首条 user 消息派生
	messages: ChatMessage[]
	updatedAt: number
}
export interface SessionStore {
	sessions: ChatSession[] // 按 updatedAt 倒序维护（最近的在前）
	activeId: string
}

const PREFIX = 'jasper.chat.'
export const MAX_SESSIONS = 50
const MAX_MESSAGES = 500 // 每会话消息上限，防 localStorage 膨胀（超出保留最近的）

export function storageKeyFor(pluginId: string, panelId: string): string {
	return `${pluginId}/${panelId}`
}

export function makeSession(id: string, now: number): ChatSession {
	return { id, title: '', messages: [], updatedAt: now }
}

export function emptyStore(id: string, now: number): SessionStore {
	return { sessions: [makeSession(id, now)], activeId: id }
}

/** 会话显示名：显式标题优先，否则取首条 user 消息首行（截断），都没有 → 空串（UI 兜「新对话」）。 */
export function sessionTitle(s: ChatSession): string {
	const explicit = s.title.trim()
	if (explicit) return explicit
	const firstUser = s.messages.find((m) => m.role === 'user')
	if (!firstUser) return ''
	const line = firstUser.content.trim().split('\n')[0]
	return line.length > 40 ? line.slice(0, 40) + '…' : line
}

export function activeSession(store: SessionStore): ChatSession {
	return store.sessions.find((s) => s.id === store.activeId) ?? store.sessions[0]
}

function isMessage(v: unknown): v is ChatMessage {
	if (!v || typeof v !== 'object') return false
	const m = v as Record<string, unknown>
	return (m.role === 'user' || m.role === 'assistant' || m.role === 'system') && typeof m.content === 'string'
}

function sanitizeSession(v: unknown, fallbackId: string): ChatSession | null {
	if (!v || typeof v !== 'object') return null
	const s = v as Record<string, unknown>
	const id = typeof s.id === 'string' && s.id ? s.id : fallbackId
	const messages = Array.isArray(s.messages) ? s.messages.filter(isMessage).slice(-MAX_MESSAGES) : []
	return {
		id,
		title: typeof s.title === 'string' ? s.title : '',
		messages,
		updatedAt: typeof s.updatedAt === 'number' ? s.updatedAt : 0,
	}
}

/** 从 localStorage 读并校验；缺失/损坏 → 单个空会话。genId/now 注入便于测试。 */
export function loadStore(key: string, genId: () => string, now: () => number): SessionStore {
	try {
		const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(PREFIX + key) : null
		if (raw) {
			const parsed = JSON.parse(raw) as Record<string, unknown>
			const sessions = Array.isArray(parsed?.sessions)
				? (parsed.sessions as unknown[])
						.map((s, i) => sanitizeSession(s, `s${i}`))
						.filter((s): s is ChatSession => s !== null)
						.slice(0, MAX_SESSIONS)
				: []
			if (sessions.length) {
				const activeId =
					typeof parsed.activeId === 'string' && sessions.some((s) => s.id === parsed.activeId)
						? parsed.activeId
						: sessions[0].id
				return { sessions, activeId }
			}
		}
	} catch {
		/* 损坏 JSON → 退回新空会话 */
	}
	return emptyStore(genId(), now())
}

export function persistStore(key: string, store: SessionStore): void {
	try {
		if (typeof localStorage !== 'undefined') localStorage.setItem(PREFIX + key, JSON.stringify(store))
	} catch {
		/* 配额满 / 隐私模式 → 忽略（会话退化为纯内存态） */
	}
}

// ---------- 不可变增删改（返回新 store） ----------

/** 往当前会话追加一条消息，刷新 updatedAt 并把该会话冒泡到最前。 */
export function appendMessage(store: SessionStore, msg: ChatMessage, now: number): SessionStore {
	const sessions = store.sessions.map((s) =>
		s.id === store.activeId ? { ...s, messages: [...s.messages, msg].slice(-MAX_MESSAGES), updatedAt: now } : s,
	)
	return { sessions: sortByRecent(sessions), activeId: store.activeId }
}

/** 新建并切到一个空会话。若当前会话已空（无消息、无标题），则复用它而不是再堆一个空的。 */
export function startSession(store: SessionStore, genId: () => string, now: number): SessionStore {
	const cur = activeSession(store)
	if (cur && cur.messages.length === 0 && !cur.title.trim()) {
		return store // 已经站在一个空会话上，直接用
	}
	const s = makeSession(genId(), now)
	return { sessions: [s, ...store.sessions].slice(0, MAX_SESSIONS), activeId: s.id }
}

export function switchSession(store: SessionStore, id: string): SessionStore {
	return store.sessions.some((s) => s.id === id) ? { ...store, activeId: id } : store
}

/** 删除一个会话；删空了则补一个空会话；删的是当前则切到最近的。 */
export function removeSession(store: SessionStore, id: string, genId: () => string, now: number): SessionStore {
	const sessions = store.sessions.filter((s) => s.id !== id)
	if (sessions.length === 0) {
		return emptyStore(genId(), now)
	}
	const activeId = store.activeId === id ? sessions[0].id : store.activeId
	return { sessions, activeId }
}

export function renameSession(store: SessionStore, id: string, title: string): SessionStore {
	const sessions = store.sessions.map((s) => (s.id === id ? { ...s, title } : s))
	return { ...store, sessions }
}

function sortByRecent(sessions: ChatSession[]): ChatSession[] {
	// 稳定：updatedAt 倒序；相等按原序（slice 保护原数组）
	return sessions.slice().sort((a, b) => b.updatedAt - a.updatedAt)
}
