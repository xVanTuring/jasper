// 轻量 i18n（无第三方包）：用 Svelte 5 rune 存当前语言，t() 取词。
// 在组件模板里调用 t() 会读到 `current`（$state），切换语言即触发重渲染。
// 纯 .ts 模块（如 api.ts）里调用则取当时的语言，足够用于一次性错误信息。
//
// 语言可运行时扩展：内置 zh/en 之外，插件经 [[contributes.locale]] 贡献语言包
// （catalog = message key → 译文），由 plugins.svelte.ts 在插件列表加载后
// registerPluginLocales() 登记。缺失的 key 回落到该语言声明的 base（en|zh）再回落内置。
// 时序与主题贡献同构：detect() 放宽（未知 code 也先接受，避免闪回），
// registerPluginLocales() 是唯一收敛点（来源插件消失 → 回落浏览器默认语言）。

import { messages, type Locale, type MsgKey } from './messages'

const STORE_KEY = 'jasper.locale'

/** 内置语言与其自称显示名（endonym，各语言里写法一致，不经 t() 翻译）。 */
const BUILTIN_LOCALES: { code: Locale; name: string }[] = [
	{ code: 'zh', name: '中文' },
	{ code: 'en', name: 'English' },
]

/** 插件贡献的一门语言（catalog 已由前端 fetch 并解析成 key→译文）。 */
export interface PluginLocale {
	code: string
	name: string
	base: Locale // 缺失键的回落语言（宿主内置之一）
	messages: Partial<Record<MsgKey, string>>
}

// 插件语言表（registerPluginLocales 维护整表替换）。
let pluginLocales = $state<PluginLocale[]>([])

function isBuiltin(code: string): code is Locale {
	return code === 'zh' || code === 'en'
}

function pluginLocaleOf(code: string): PluginLocale | undefined {
	return pluginLocales.find((p) => p.code === code)
}

function isKnownLocale(code: string): boolean {
	return isBuiltin(code) || pluginLocaleOf(code) !== undefined
}

/** 无保存值时的默认语言：浏览器语言以 zh 开头 → zh，否则 en。 */
function browserDefault(): Locale {
	try {
		return navigator.language.toLowerCase().startsWith('zh') ? 'zh' : 'en'
	} catch {
		return 'en'
	}
}

function detect(): string {
	try {
		const saved = localStorage.getItem(STORE_KEY)
		// 放宽：任何非空保存值（可能是尚未注册的插件语言 code）都先接受，收敛交给 registerPluginLocales
		if (saved) return saved
	} catch {
		/* localStorage 不可用时忽略 */
	}
	return browserDefault()
}

let current = $state<string>(detect())

export function getLocale(): string {
	return current
}

export function setLocale(l: string) {
	current = l
	try {
		localStorage.setItem(STORE_KEY, l)
	} catch {
		/* 忽略 */
	}
}

/** 顶栏小按钮/测试用：在内置 zh/en 间翻转（插件语言经语言选择器切换）。 */
export function toggleLocale() {
	setLocale(current === 'zh' ? 'en' : 'zh')
}

/** 可选语言列表（内置 + 插件；插件不得顶替内置 zh/en），供语言选择器渲染。 */
export function availableLocales(): { code: string; name: string }[] {
	const out: { code: string; name: string }[] = BUILTIN_LOCALES.map((b) => ({ ...b }))
	for (const p of pluginLocales) {
		if (isBuiltin(p.code)) continue
		out.push({ code: p.code, name: p.name })
	}
	return out
}

/** 语言显示名（内置返回 endonym，插件返回其声明名，未知回落 code 本身）。 */
export function localeName(code: string): string {
	const b = BUILTIN_LOCALES.find((x) => x.code === code)
	if (b) return b.name
	return pluginLocaleOf(code)?.name ?? code
}

/**
 * 登记插件语言（整表替换；插件启停/卸载后由 plugins.svelte.ts 重新调用）。
 * 收敛点：当前选择的语言来源插件已消失（不再是已知语言）→ 回落浏览器默认语言。
 */
export function registerPluginLocales(list: PluginLocale[]) {
	pluginLocales = list
	if (!isKnownLocale(current)) setLocale(browserDefault())
}

/** 在某语言里查一个 key：内置直查；插件语言查 catalog，缺失回落其 base。 */
function lookup(code: string, key: MsgKey): string | undefined {
	if (isBuiltin(code)) return messages[code][key]
	const pl = pluginLocaleOf(code)
	if (!pl) return undefined
	return pl.messages[key] ?? messages[pl.base][key]
}

/** 取词并按 {name} 插值。回落链：当前语言 → 其 base → en → zh → 键名本身。 */
export function t(key: MsgKey, params?: Record<string, string | number>): string {
	let s = lookup(current, key) ?? messages.en[key] ?? messages.zh[key] ?? key
	if (params) {
		for (const k in params) s = s.split(`{${k}}`).join(String(params[k]))
	}
	return s
}
