// 设置面板壳（服务器驱动）：分区侧边栏 + 搜索 + 客户端本地分区（外观/编辑器）。
// 只走读/客户端本地路径，不改服务端状态（不设密码、不切数据源），避免污染其它 spec。
import { test, expect } from '@playwright/test'

test('设置面板：分区侧边栏 + 搜索 + 客户端分区切换', async ({ page }) => {
	await page.goto('/')
	await page.getByRole('button', { name: /^(设置|Settings)$/ }).click()

	// 侧边栏分区（默认构建至少含这四个；AI 段视 plugins feature 而定，不在此断言）
	for (const name of [
		/^(数据源|Data source)$/,
		/^(访问控制|Access control)$/,
		/^(外观|Appearance)$/,
		/^(编辑器|Editor)$/,
	]) {
		await expect(page.getByRole('button', { name })).toBeVisible()
	}

	// 搜索：无匹配 → 提示；清空 → 恢复
	const search = page.getByPlaceholder(/搜索设置|Search settings/)
	await search.fill('zzzznomatch')
	await expect(page.getByText(/没有匹配|No matching/)).toBeVisible()
	await search.fill('')

	// 访问控制分区：Save 按钮应可用（回归护栏——非数据源分区不得被 prefillMissing 误禁用）
	await page.getByRole('button', { name: /^(访问控制|Access control)$/ }).click()
	await expect(page.getByRole('button', { name: /保存访问控制|Save access control/ })).toBeEnabled()

	// 外观分区：语言按钮可见（客户端本地）
	await page.getByRole('button', { name: /^(外观|Appearance)$/ }).click()
	await expect(page.getByRole('button', { name: 'English' })).toBeVisible()

	// 编辑器分区：切换默认引擎（写 localStorage，客户端本地无副作用），验证选中态
	await page.getByRole('button', { name: /^(编辑器|Editor)$/ }).click()
	const wysiwyg = page.getByRole('button', { name: /^(实时预览|Live preview)$/ })
	await wysiwyg.click()
	await expect(wysiwyg).toHaveClass(/on/)
})
