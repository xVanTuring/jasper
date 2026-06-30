// 轻量 i18n（无第三方包）：用 Svelte 5 rune 存当前语言，t() 取词。
// 在组件模板里调用 t() 会读到 `current`（$state），切换语言即触发重渲染。
// 纯 .ts 模块（如 api.ts）里调用则取当时的语言，足够用于一次性错误信息。

import { messages, type Locale, type MsgKey } from './messages'

const STORE_KEY = 'jasper.locale'

function detect(): Locale {
	try {
		const saved = localStorage.getItem(STORE_KEY)
		if (saved === 'zh' || saved === 'en') return saved
	} catch {
		/* localStorage 不可用时忽略 */
	}
	return navigator.language.toLowerCase().startsWith('zh') ? 'zh' : 'en'
}

let current = $state<Locale>(detect())

export function getLocale(): Locale {
	return current
}

export function setLocale(l: Locale) {
	current = l
	try {
		localStorage.setItem(STORE_KEY, l)
	} catch {
		/* 忽略 */
	}
}

export function toggleLocale() {
	setLocale(current === 'zh' ? 'en' : 'zh')
}

/** 取词并按 {name} 插值。缺失时回退到 zh，再回退到键名本身。 */
export function t(key: MsgKey, params?: Record<string, string | number>): string {
	let s = messages[current][key] ?? messages.zh[key] ?? key
	if (params) {
		for (const k in params) s = s.split(`{${k}}`).join(String(params[k]))
	}
	return s
}
