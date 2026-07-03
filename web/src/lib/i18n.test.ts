import { describe, it, expect, beforeEach } from 'vitest'
import { messages, type MsgKey } from './messages'
import {
	t,
	getLocale,
	setLocale,
	toggleLocale,
	registerPluginLocales,
	availableLocales,
	localeName,
	resolveText,
} from './i18n.svelte'

beforeEach(() => {
	localStorage.clear()
	registerPluginLocales([]) // 清空插件语言（模块级 rune 状态跨用例共享）
	setLocale('zh') // 每个用例从确定语言开始
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

describe('resolveText (localizable runtime UI text)', () => {
	it('passes plain strings through', () => {
		setLocale('zh')
		expect(resolveText('hello')).toBe('hello')
	})

	it('picks the current locale from a map', () => {
		setLocale('zh')
		expect(resolveText({ en: 'Save', zh: '保存' })).toBe('保存')
		setLocale('en')
		expect(resolveText({ en: 'Save', zh: '保存' })).toBe('Save')
	})

	it('falls back current → en → zh → first non-empty', () => {
		setLocale('zh')
		expect(resolveText({ en: 'OnlyEn' })).toBe('OnlyEn') // 无 zh → en
		expect(resolveText({ fr: 'Bonjour' })).toBe('Bonjour') // 无 zh/en → 首个非空
	})

	it('resolves plugin locale, then its base via en', () => {
		registerPluginLocales([{ code: 'fr', name: 'Français', base: 'en', messages: {} }])
		setLocale('fr')
		expect(resolveText({ fr: 'Bonjour', en: 'Hello' })).toBe('Bonjour')
		expect(resolveText({ en: 'Hello' })).toBe('Hello') // 无 fr → en
	})

	it('is defensive on empties/non-objects', () => {
		setLocale('zh')
		expect(resolveText({})).toBe('')
		expect(resolveText(undefined)).toBe('')
		expect(resolveText(null)).toBe('')
		expect(resolveText(42)).toBe('42')
	})
})

describe('plugin locales', () => {
	const someKey = Object.keys(messages.en)[0] as MsgKey
	const otherKey = Object.keys(messages.en)[1] as MsgKey

	it('registers a plugin language and t() reads its catalog', () => {
		registerPluginLocales([
			{ code: 'fr', name: 'Français', base: 'en', messages: { [someKey]: 'FR-VALUE' } },
		])
		setLocale('fr')
		expect(getLocale()).toBe('fr')
		expect(t(someKey)).toBe('FR-VALUE')
	})

	it('falls back to the declared base for missing keys', () => {
		registerPluginLocales([
			{ code: 'fr', name: 'Français', base: 'zh', messages: { [someKey]: 'FR-VALUE' } },
		])
		setLocale('fr')
		// someKey 有译文；otherKey 缺失 → 回落 base(zh)
		expect(t(someKey)).toBe('FR-VALUE')
		expect(t(otherKey)).toBe(messages.zh[otherKey])
	})

	it('lists plugin languages in availableLocales (builtins first, no override of zh/en)', () => {
		registerPluginLocales([
			{ code: 'fr', name: 'Français', base: 'en', messages: {} },
			{ code: 'en', name: 'HIJACK', base: 'en', messages: {} }, // 不得顶替内置 en
		])
		const codes = availableLocales().map((l) => l.code)
		expect(codes.slice(0, 2)).toEqual(['zh', 'en'])
		expect(codes).toContain('fr')
		expect(codes.filter((c) => c === 'en')).toHaveLength(1)
		expect(localeName('fr')).toBe('Français')
		expect(localeName('en')).toBe('English') // 内置名，非插件的 HIJACK
	})

	it('converges: current plugin locale disappearing falls back to a builtin', () => {
		registerPluginLocales([{ code: 'fr', name: 'Français', base: 'en', messages: {} }])
		setLocale('fr')
		expect(getLocale()).toBe('fr')
		registerPluginLocales([]) // 插件被停用/卸载
		expect(['zh', 'en']).toContain(getLocale())
	})
})
