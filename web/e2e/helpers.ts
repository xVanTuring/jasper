import type { Page } from '@playwright/test'

// 固定语言为英文，让按钮/占位符文案确定；可选预置编辑器引擎。
// addInitScript 在每次导航前、页面脚本前运行 → localStorage 先就位。
export async function setupApp(page: Page, opts: { editor?: 'live' | 'source' } = {}) {
	const editor = opts.editor
	await page.addInitScript(
		([ed]) => {
			localStorage.setItem('jasper.locale', 'en')
			if (ed) localStorage.setItem('jasper.editor', ed)
		},
		[editor],
	)
}

// 打开应用并等库加载（首个笔记本自动选中 → 笔记列表就绪）。
export async function openApp(page: Page, opts?: { editor?: 'live' | 'source' }) {
	await setupApp(page, opts)
	await page.goto('/')
}

// 打开一篇笔记并切到「阅读」视图（.content）。默认打开即进编辑态，
// 故校验阅读渲染的测试需显式切回阅读。
export async function openNoteRead(page: Page, name: string) {
	await page.locator('button.note', { hasText: name }).click()
	await page.getByRole('button', { name: 'Read', exact: true }).click()
	await page.locator('.content').first().waitFor()
}
