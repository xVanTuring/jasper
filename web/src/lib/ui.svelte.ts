// 全局界面偏好：按钮显示模式（图标+文字 / 仅图标 / 仅文字），rune + localStorage。
// 让用户/插件主题后续可统一控制“按钮显示图标还是文字”。Button.svelte 读它决定渲染。

export type ButtonDisplay = 'both' | 'icon' | 'text'

const STORE_KEY = 'joplin-lite.btn-display'

function load(): ButtonDisplay {
	try {
		const s = localStorage.getItem(STORE_KEY)
		if (s === 'both' || s === 'icon' || s === 'text') return s
	} catch {
		/* 忽略 */
	}
	return 'both'
}

let mode = $state<ButtonDisplay>(load())

export function getButtonDisplay(): ButtonDisplay {
	return mode
}

export function setButtonDisplay(m: ButtonDisplay) {
	mode = m
	try {
		localStorage.setItem(STORE_KEY, m)
	} catch {
		/* 忽略 */
	}
}
