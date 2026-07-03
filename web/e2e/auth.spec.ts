import { test, expect } from '@playwright/test'
import { openApp } from './helpers'

// 访问鉴权 e2e：后端初始无密码（server.mjs 每次启动清 config 目录）。
// e2e 单 worker 串行（playwright.config：fullyParallel:false, workers:1），故只要每例
// 用 afterEach 清掉密码，就不会污染其它 spec。

const PW = 'letmein'

test.afterEach(async ({ page }) => {
	// 用已知密码登录取 token → 清除访问密码，恢复全开放（幂等；没设密码则登录失败直接跳过）。
	const login = await page.request.post('/api/auth/login', { data: { password: PW } })
	if (!login.ok()) return
	const { token } = await login.json()
	await page.request.put('/api/auth/settings', {
		headers: { authorization: `Bearer ${token}` },
		data: { clear_password: true, passwordless_read: false, list_mode: 'none', folder_list: [] },
	})
})

test('password makes anonymous read-only; login unlocks writes', async ({ page }) => {
	await openApp(page)
	await expect(page.locator('button.folder', { hasText: 'Notebook' })).toBeVisible()
	// 尚未设密码：无解锁入口
	await expect(page.getByRole('button', { name: 'Unlock (log in)', exact: true })).toHaveCount(0)

	// 经 API 设访问密码 + 允许无密码阅读（后端初始开放 → 允许）
	const res = await page.request.put('/api/auth/settings', {
		data: { password: PW, passwordless_read: true, list_mode: 'none', folder_list: [] },
	})
	expect(res.ok()).toBeTruthy()

	// 重载 → 浏览器无 token → 匿名只读；passwordless 开 → 仍能看到笔记本
	await page.reload()
	await expect(page.locator('button.folder', { hasText: 'Notebook' })).toBeVisible()
	await expect(page.locator('.ro-badge')).toBeVisible()
	const unlock = page.getByRole('button', { name: 'Unlock (log in)', exact: true })
	await expect(unlock).toBeVisible()

	// 登录
	await unlock.click()
	await page.getByPlaceholder('Access password').fill(PW)
	await page.getByRole('button', { name: 'Log in', exact: true }).click()

	// 登录后：只读徽标消失、出现登出按钮
	await expect(page.locator('.ro-badge')).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Lock (log out)', exact: true })).toBeVisible()
	await expect(page.locator('button.folder', { hasText: 'Notebook' })).toBeVisible()
})

test('wrong password shows an error and stays locked', async ({ page }) => {
	await openApp(page)
	const res = await page.request.put('/api/auth/settings', {
		data: { password: PW, passwordless_read: true, list_mode: 'none', folder_list: [] },
	})
	expect(res.ok()).toBeTruthy()
	await page.reload()

	await page.getByRole('button', { name: 'Unlock (log in)', exact: true }).click()
	await page.getByPlaceholder('Access password').fill('wrong-password')
	await page.getByRole('button', { name: 'Log in', exact: true }).click()

	await expect(page.getByText('Wrong password')).toBeVisible()
	await expect(page.locator('.ro-badge')).toBeVisible()
})
