// autosave.ts：自动保存协调器（脏标记/在途/快照复核/外部回显门控）纯逻辑单测。
// 重点回归「编辑到一半被重置」：保存 await 期间用户续打字时，绝不能把缓冲当作已保存，
// 也绝不能让外部回显覆盖更晚的输入。
import { describe, it, expect } from 'vitest'
import { Autosave, type NoteSnapshot } from './autosave'

const snap = (title: string, body: string): NoteSnapshot => ({ title, body })

describe('Autosave', () => {
	it('初始态：不脏、不在途、不可保存', () => {
		const a = new Autosave()
		expect(a.dirty).toBe(false)
		expect(a.saving).toBe(false)
		expect(a.canBeginSave()).toBe(false)
	})

	it('markDirty 后可发起保存', () => {
		const a = new Autosave()
		a.markDirty()
		expect(a.dirty).toBe(true)
		expect(a.canBeginSave()).toBe(true)
	})

	it('在途保存期间不允许并发再发起', () => {
		const a = new Autosave()
		a.markDirty()
		a.beginSave()
		expect(a.saving).toBe(true)
		expect(a.canBeginSave()).toBe(false) // 已在途 → 不并发
	})

	it('保存期间缓冲未变：转干净', () => {
		const a = new Autosave()
		a.markDirty()
		a.beginSave()
		const saved = snap('t', 'A')
		// 返回时缓冲仍等于发出的快照
		expect(a.finishSaveOk(snap('t', 'A'), saved)).toBe(true)
		expect(a.dirty).toBe(false)
		expect(a.saving).toBe(false)
	})

	it('保存期间又改动：保持脏、需再存', () => {
		const a = new Autosave()
		a.markDirty()
		a.beginSave()
		const saved = snap('t', 'A')
		// 返回时缓冲已是更晚的输入（用户在 await 期间续打字）
		expect(a.finishSaveOk(snap('t', 'AB'), saved)).toBe(false)
		expect(a.dirty).toBe(true) // 仍脏
		expect(a.saving).toBe(false)
		expect(a.canBeginSave()).toBe(true) // 可再排一次保存
	})

	it('保存失败：退出在途但保持脏（内容仍未落盘）', () => {
		const a = new Autosave()
		a.markDirty()
		a.beginSave()
		a.finishSaveErr()
		expect(a.dirty).toBe(true)
		expect(a.saving).toBe(false)
	})

	describe('canApplyExternal（外部回显门控）', () => {
		it('干净且内容不同：允许回显', () => {
			const a = new Autosave()
			expect(a.canApplyExternal(snap('t', 'A'), snap('t', 'B'))).toBe(true)
		})

		it('内容相同：无需回显', () => {
			const a = new Autosave()
			expect(a.canApplyExternal(snap('t', 'A'), snap('t', 'A'))).toBe(false)
		})

		it('有未保存输入：拒绝回显（不打断用户）', () => {
			const a = new Autosave()
			a.markDirty()
			expect(a.canApplyExternal(snap('t', 'AB'), snap('t', 'A'))).toBe(false)
		})

		it('正在保存：拒绝回显', () => {
			const a = new Autosave()
			a.markDirty()
			a.beginSave()
			expect(a.canApplyExternal(snap('t', 'A'), snap('t', 'X'))).toBe(false)
		})
	})

	// 回归：完整复现「编辑到一半被重置」的时序，断言外部回声不会重置缓冲。
	it('回归：保存 await 期间续打字时，自己写入的 SSE 回声被拒绝（不重置输入与光标）', () => {
		const a = new Autosave()

		// 1) 打字 "A" → 置脏；防抖到点发起保存，对发出内容取快照 "A"
		a.markDirty()
		expect(a.canBeginSave()).toBe(true)
		a.beginSave()
		const sent = snap('t', 'A')

		// 2) await 期间用户续打字 "AB"（缓冲已领先服务端）
		a.markDirty()
		let buffer = snap('t', 'AB')

		// 3) 保存("A")返回：缓冲已变 → 不清脏、需再存（旧实现在此误清 dirty 是根因）
		expect(a.finishSaveOk(buffer, sent)).toBe(false)
		expect(a.dirty).toBe(true)

		// 4) 自己写入触发的 SSE 回声带回服务端旧值 "A" → 必须拒绝，否则会 setValue 重置缓冲
		const echoFromServer = snap('t', 'A')
		expect(a.canApplyExternal(buffer, echoFromServer)).toBe(false)

		// 5) 稍后再保存 "AB" 成功且期间无新输入 → 转干净；之后同值回声成为 no-op
		a.beginSave()
		buffer = snap('t', 'AB')
		expect(a.finishSaveOk(buffer, snap('t', 'AB'))).toBe(true)
		expect(a.dirty).toBe(false)
		expect(a.canApplyExternal(buffer, snap('t', 'AB'))).toBe(false) // 同值，no-op
	})
})
