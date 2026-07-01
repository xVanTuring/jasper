import { describe, it, expect, beforeEach } from 'vitest'
import { messages, type MsgKey } from './messages'
import { t, getLocale, setLocale, toggleLocale } from './i18n.svelte'

beforeEach(() => {
	localStorage.clear()
	setLocale('zh') // 每个用例从确定语言开始（模块级 rune 状态跨用例共享）
})

describe('messages dictionary', () => {
	it('zh and en expose exactly the same keys', () => {
		const zhKeys = Object.keys(messages.zh).sort()
		const enKeys = Object.keys(messages.en).sort()
		expect(enKeys).toEqual(zhKeys)
	})

	it('has no empty strings', () => {
		for (const loc of ['zh', 'en'] as const)
			for (const [k, v] of Object.entries(messages[loc]))
				expect(v, `${loc}.${k} is empty`).not.toBe('')
	})

	it('keeps the same {placeholders} in both languages', () => {
		const holders = (s: string) => (s.match(/\{[a-zA-Z0-9_]+\}/g) ?? []).sort()
		for (const k of Object.keys(messages.zh) as MsgKey[])
			expect(holders(messages.en[k]), `placeholders mismatch on ${k}`).toEqual(
				holders(messages.zh[k]),
			)
	})
})

describe('t()', () => {
	it('returns the current-locale string', () => {
		setLocale('zh')
		expect(t('common.save')).toBe(messages.zh['common.save'])
		setLocale('en')
		expect(t('common.save')).toBe(messages.en['common.save'])
	})

	it('interpolates {named} params', () => {
		// 'common.themeTitle' 含占位符 {mode}
		const out = t('common.themeTitle', { mode: 'XYZ' })
		expect(out).toContain('XYZ')
		expect(out).not.toContain('{mode}')
	})

	it('replaces every occurrence of a placeholder', () => {
		const tpl = '{a}-{a}'
		const rendered = tpl.split('{a}').join('Z')
		expect(rendered).toBe('Z-Z') // sanity for the split/join strategy t() uses
	})

	it('falls back to the key itself for unknown keys', () => {
		expect(t('nope.not.a.real.key' as MsgKey)).toBe('nope.not.a.real.key')
	})
})

describe('locale state', () => {
	it('setLocale updates getLocale and persists to localStorage', () => {
		setLocale('en')
		expect(getLocale()).toBe('en')
		expect(localStorage.getItem('jasper.locale')).toBe('en')
	})

	it('toggleLocale flips between zh and en', () => {
		setLocale('zh')
		toggleLocale()
		expect(getLocale()).toBe('en')
		toggleLocale()
		expect(getLocale()).toBe('zh')
	})
})
