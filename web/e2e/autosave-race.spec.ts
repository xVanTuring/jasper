import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { openApp } from './helpers'
import { IDS } from './make-fixture.mjs'
import { DATA_DIR } from './config'

// 回归：「编辑到一半被重置」（笔记与光标恢复）。
// 成因是一次自动保存 await 期间用户继续打字，保存返回后无条件清 dirty，把含新输入的缓冲误判为
// 已保存；随后自己写入触发的 SSE 回声经 NoteView.applyExternal 用服务端旧值 setValue 整篇替换缓冲
// → 更晚的输入与光标被重置。修复见 autosave.ts / NoteView.svelte（保存前快照 + 复核 + 回显门控）。
//
// 该 bug 本质是竞态，直接靠真实时序复现会 flaky。这里用 page.route 精确控制第一次 PUT 的时序：
// 请求照常打到真后端（服务端落盘 + 广播 SSE），但把响应「扣住」不回给前端，直到我们已敲入第二段，
// 从而稳定地构造出「保存 await 期间续打字」的窗口。
test('typing during an in-flight autosave is not reset by the self SSE echo', async ({
	page,
	request,
}) => {
	await openApp(page, { editor: 'source' })
	await page.locator('button.note', { hasText: 'Plain Note' }).click()

	const cm = page.locator('.cm-content')
	await expect(cm).toBeVisible()

	// 扣住第一次 PUT 的响应，直到测试主动放行。
	let releaseFirst!: () => void
	const firstReleased = new Promise<void>((r) => (releaseFirst = r))
	let markFirstSeen!: () => void
	const firstPutSeen = new Promise<void>((r) => (markFirstSeen = r))
	let putCount = 0

	await page.route(`**/api/notes/${IDS.plainNote}`, async (route) => {
		if (route.request().method() !== 'PUT') return route.continue()
		putCount++
		if (putCount === 1) {
			// 让请求真正打到后端：后端此刻已落盘 PART-ONE 并广播 SSE 变更事件。
			const resp = await route.fetch()
			markFirstSeen()
			await firstReleased // 扣住响应——前端仍处于 saving/await 态
			return route.fulfill({ response: resp })
		}
		return route.continue() // 后续保存（PART-TWO 落盘）照常放行
	})

	// 1) 敲第一段并停顿 → 触发自动保存（PUT#1，被 route 扣在途）。
	await cm.click()
	await page.keyboard.press('End')
	await page.keyboard.type(' PART-ONE')
	await firstPutSeen // PUT#1 已到后端、SSE 已广播、响应被扣住

	// 2) 在 PUT#1 在途期间敲第二段（旧实现会在 PUT#1 返回时误清 dirty）。
	await page.keyboard.type(' PART-TWO')

	// 3) 放行 PUT#1 → 前端 await 返回（旧实现此刻 dirty=false）。随后自身 SSE 回声到达，
	//    旧实现经 applyExternal 用服务端旧值（仅含 PART-ONE）setValue 整篇替换 → 重置掉 PART-TWO。
	releaseFirst()

	// 给回声 + 去抖（App.svelte 250ms）+ 一次 GET 一点时间，让潜在的重置有机会发生。
	// 修复后缓冲全程保留两段、绝不被回声重置。
	await page.waitForTimeout(1000)
	await expect(cm).toContainText('PART-ONE')
	await expect(cm).toContainText('PART-TWO')

	// 4) 两段最终都持久化（PART-TWO 经后续保存落盘，未丢失）。
	await expect
		.poll(
			async () => (await (await request.get(`/api/notes/${IDS.plainNote}`)).json()).body,
			{ timeout: 5000 },
		)
		.toContain('PART-TWO')

	const onDisk = readFileSync(join(DATA_DIR, `${IDS.plainNote}.md`), 'utf8')
	expect(onDisk).toContain('PART-ONE')
	expect(onDisk).toContain('PART-TWO')

	await page.unroute(`**/api/notes/${IDS.plainNote}`)
})
