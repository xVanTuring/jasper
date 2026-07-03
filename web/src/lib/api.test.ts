import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import {
	parseResourceId,
	taskProgress,
	api,
	authHeaders,
	setAuthToken,
	getAuthToken,
	setAuthErrorHandler,
} from './api'

const ID = '0123456789abcdef0123456789abcdef' // 32 hex

describe('parseResourceId', () => {
	it('parses the `:/id` form', () => {
		expect(parseResourceId(`:/${ID}`)).toBe(ID)
	})

	it('parses the `joplin://id` form', () => {
		expect(parseResourceId(`joplin://${ID}`)).toBe(ID)
	})

	it('accepts alphanumeric ids and trims surrounding whitespace', () => {
		const mixed = 'aZ09'.repeat(8) // 32 alphanumerics
		expect(parseResourceId(`  :/${mixed}  `)).toBe(mixed)
	})

	it('ignores a trailing #anchor or ?query', () => {
		expect(parseResourceId(`:/${ID}#section`)).toBe(ID)
		expect(parseResourceId(`:/${ID}?x=1`)).toBe(ID)
	})

	it('returns null for non-resource urls and malformed ids', () => {
		expect(parseResourceId('https://example.com/a.png')).toBeNull()
		expect(parseResourceId(`:/${ID}extra`)).toBeNull() // 37 chars
		expect(parseResourceId(`:/${ID.slice(0, 31)}`)).toBeNull() // 31 chars
		expect(parseResourceId(':/../etc/passwd')).toBeNull()
		expect(parseResourceId('')).toBeNull()
	})
})

describe('taskProgress', () => {
	it('counts done/total across GFM checkboxes', () => {
		const body = ['- [ ] a', '- [x] b', '- [X] c'].join('\n')
		expect(taskProgress(body)).toEqual([2, 3])
	})

	it('accepts *, + and - bullets and leading indentation', () => {
		const body = ['* [ ] a', '  + [x] nested', '- [ ] c'].join('\n')
		expect(taskProgress(body)).toEqual([1, 3])
	})

	it('ignores non-task lines and prose', () => {
		const body = ['# Title', 'some text', '- a plain item', '[ ] not a bullet'].join('\n')
		expect(taskProgress(body)).toEqual([0, 0])
	})

	it('returns [0,0] for empty/blank input', () => {
		expect(taskProgress('')).toEqual([0, 0])
		expect(taskProgress('\n\n')).toEqual([0, 0])
	})
})

describe('auth token (access control)', () => {
	beforeEach(() => setAuthToken(null))
	afterEach(() => {
		setAuthToken(null)
		setAuthErrorHandler(null)
		vi.restoreAllMocks()
		vi.unstubAllGlobals()
	})

	it('authHeaders omits Authorization when no token, adds Bearer when set', () => {
		expect(authHeaders()).toEqual({})
		expect(authHeaders({ 'Content-Type': 'x' })).toEqual({ 'Content-Type': 'x' })
		setAuthToken('tok123')
		expect(getAuthToken()).toBe('tok123')
		expect(authHeaders()).toEqual({ Authorization: 'Bearer tok123' })
		expect(authHeaders({ 'Content-Type': 'x' })).toEqual({
			'Content-Type': 'x',
			Authorization: 'Bearer tok123',
		})
	})

	it('login stores the token; later requests carry it as Bearer', async () => {
		const fetchMock = vi
			.fn()
			.mockResolvedValueOnce({ ok: true, status: 200, json: async () => ({ token: 'sess-1' }) })
			.mockResolvedValueOnce({ ok: true, status: 200, json: async () => ({}) })
		vi.stubGlobal('fetch', fetchMock)

		expect(await api.login('pw')).toBe(true)
		expect(getAuthToken()).toBe('sess-1')

		await api.getConfig()
		const init = fetchMock.mock.calls[1][1] as RequestInit
		expect((init.headers as Record<string, string>).Authorization).toBe('Bearer sess-1')
	})

	it('login returns false on wrong password (401) and stores no token', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({ ok: false, status: 401, json: async () => ({}) }),
		)
		expect(await api.login('wrong')).toBe(false)
		expect(getAuthToken()).toBeNull()
	})

	it('logout clears the local token', async () => {
		setAuthToken('sess-2')
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({ ok: true, status: 204, json: async () => ({}) }),
		)
		await api.logout()
		expect(getAuthToken()).toBeNull()
	})

	it('a 401 on a read clears the token and fires the auth-error handler', async () => {
		setAuthToken('stale')
		const handler = vi.fn()
		setAuthErrorHandler(handler)
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({ ok: false, status: 401, json: async () => ({}) }),
		)
		await expect(api.getConfig()).rejects.toThrow()
		expect(getAuthToken()).toBeNull()
		expect(handler).toHaveBeenCalledOnce()
	})
})
