// 主题：用 Svelte 5 rune 存当前选择，应用到 <html data-theme>，localStorage 持久化。
// 选择可为内置 auto|light|dark、内置自定义主题（CUSTOM_THEMES），或插件贡献的主题
// （由 plugins.svelte.ts 在插件列表加载后 registerPluginThemes 登记）。
// auto 跟随系统并实时更新；自定义主题各自声明 base(light|dark) 供编辑器明暗参考。
// 首屏无闪烁由 index.html 头部内联脚本负责（同一 localStorage 键）；这里是运行时来源。
//
// 插件主题的两个时序约定：
// - load() 放宽校验：未知 id 也先照常应用 data-theme（插件 CSS 是异步 <link>，
//   先挂属性可避免「先回落再跳变」的闪烁）；
// - registerPluginThemes() 是唯一的收敛点：插件列表就绪后仍未知的 id 才回落 auto。

export type ThemeSetting = string // 'auto' | 'light' | 'dark' | 自定义/插件主题 id

export interface CustomTheme {
	id: string
	name: string
	base: 'light' | 'dark'
}

// 内置示例主题（CSS 在 src/themes/*.css，随包发布）。
export const CUSTOM_THEMES: CustomTheme[] = [
	{ id: 'nord', name: 'Nord', base: 'dark' },
	{ id: 'solarized', name: 'Solarized Light', base: 'light' },
]

const BUILTINS = ['auto', 'light', 'dark']
const STORE_KEY = 'jasper.theme'

// 插件贡献的主题表（registerPluginThemes 维护；CSS <link> 由 plugins.svelte.ts 注入）
let pluginThemeList = $state<CustomTheme[]>([])

function allCustom(): CustomTheme[] {
	return [...CUSTOM_THEMES, ...pluginThemeList]
}

function isKnown(s: string): boolean {
	return BUILTINS.includes(s) || allCustom().some((t) => t.id === s)
}

function load(): ThemeSetting {
	try {
		const s = localStorage.getItem(STORE_KEY)
		// 放宽：未知 id（可能是尚未加载的插件主题）也接受，收敛交给 registerPluginThemes
		if (s) return s
	} catch {
		/* localStorage 不可用时忽略 */
	}
	return 'auto'
}

const darkQuery =
	typeof matchMedia !== 'undefined' ? matchMedia('(prefers-color-scheme: dark)') : null

// 写到 <html data-theme> 的值：auto → 跟随系统的 light/dark；其余原样（light/dark/自定义 id）。
function attrOf(s: ThemeSetting): string {
	if (s === 'auto') return darkQuery?.matches ? 'dark' : 'light'
	return s
}

function apply(s: ThemeSetting) {
	document.documentElement.setAttribute('data-theme', attrOf(s))
}

const initial = load()
let current = $state<ThemeSetting>(initial)
apply(initial) // 与内联脚本一致；内联失败时这里兜底

// 系统主题变化：仅当处于 auto 时跟随
darkQuery?.addEventListener('change', () => {
	if (current === 'auto') apply('auto')
})

export function getTheme(): ThemeSetting {
	return current
}

/** 当前实际生效的明暗（auto 按系统、自定义按其 base 解析）。供编辑器等需要明暗布尔的地方用。 */
export function resolvedTheme(): 'light' | 'dark' {
	if (current === 'light' || current === 'dark') return current
	if (current === 'auto') return darkQuery?.matches ? 'dark' : 'light'
	return allCustom().find((t) => t.id === current)?.base ?? 'light'
}

export function setTheme(s: ThemeSetting) {
	current = s
	try {
		localStorage.setItem(STORE_KEY, s)
	} catch {
		/* 忽略 */
	}
	apply(s)
}

/**
 * 登记插件主题（整表替换；插件启停/卸载后由 plugins.svelte.ts 重新调用）。
 * 收敛点：插件列表已知后，当前选择仍未知（来源插件被停用/卸载）→ 回落 auto。
 */
export function registerPluginThemes(themes: CustomTheme[]) {
	pluginThemeList = themes
	if (!isKnown(current)) setTheme('auto')
}

/** 可选主题 id 列表（内置 + 自定义 + 插件），供主题选择器渲染。 */
export function themeIds(): ThemeSetting[] {
	return [...BUILTINS, ...allCustom().map((t) => t.id)]
}

/** 自定义/插件主题的显示名（内置 auto/light/dark 返回 undefined，由调用方用 i18n 取名）。 */
export function customThemeName(id: string): string | undefined {
	return allCustom().find((t) => t.id === id)?.name
}
