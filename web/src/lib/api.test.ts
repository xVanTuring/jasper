import { describe, it, expect } from 'vitest'
import { parseResourceId, taskProgress } from './api'

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
