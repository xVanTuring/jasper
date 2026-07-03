// NoteTags：笔记标签行（加载/打标签/去标签/只读）。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, fireEvent } from '@testing-library/svelte'
import { tick } from 'svelte'

const { noteTags, addNoteTag, removeNoteTag } = vi.hoisted(() => ({
	noteTags: vi.fn(),
	addNoteTag: vi.fn(),
	removeNoteTag: vi.fn(),
}))
vi.mock('./api', () => ({ api: { noteTags, addNoteTag, removeNoteTag } }))

import NoteTags from './NoteTags.svelte'

describe('NoteTags', () => {
	beforeEach(() => {
		noteTags.mockReset()
		addNoteTag.mockReset()
		removeNoteTag.mockReset()
	})

	it('挂载后加载并渲染标签 chips', async () => {
		noteTags.mockResolvedValue([
			{ id: 'a', title: 'work' },
			{ id: 'b', title: 'idea' },
		])
		const { container } = render(NoteTags, { props: { noteId: 'n1' } })
		await vi.waitFor(() => expect(container.querySelectorAll('.chip').length).toBe(2))
		expect(noteTags).toHaveBeenCalledWith('n1')
		expect(container.textContent).toContain('work')
	})

	it('回车添加标签 → addNoteTag(noteId, title)，用返回值更新 + 清空输入 + onChanged', async () => {
		noteTags.mockResolvedValue([])
		addNoteTag.mockResolvedValue([{ id: 'a', title: 'new' }])
		const onChanged = vi.fn()
		const { container } = render(NoteTags, { props: { noteId: 'n1', onChanged } })
		await tick()
		const input = container.querySelector('input.add') as HTMLInputElement
		await fireEvent.input(input, { target: { value: '  new ' } })
		await fireEvent.keyDown(input, { key: 'Enter' })
		await vi.waitFor(() => expect(container.querySelector('.chip')).toBeTruthy())
		expect(addNoteTag).toHaveBeenCalledWith('n1', 'new') // 已 trim
		expect(onChanged).toHaveBeenCalled()
		expect((container.querySelector('input.add') as HTMLInputElement).value).toBe('')
	})

	it('空/纯空白输入回车不发请求', async () => {
		noteTags.mockResolvedValue([])
		const { container } = render(NoteTags, { props: { noteId: 'n1' } })
		await tick()
		const input = container.querySelector('input.add') as HTMLInputElement
		await fireEvent.input(input, { target: { value: '   ' } })
		await fireEvent.keyDown(input, { key: 'Enter' })
		await tick()
		expect(addNoteTag).not.toHaveBeenCalled()
	})

	it('点 chip 的 × 移除 → removeNoteTag(noteId, tagId) + onChanged', async () => {
		noteTags.mockResolvedValue([{ id: 'a', title: 'work' }])
		removeNoteTag.mockResolvedValue([])
		const onChanged = vi.fn()
		const { container } = render(NoteTags, { props: { noteId: 'n1', onChanged } })
		await vi.waitFor(() => expect(container.querySelector('.chip')).toBeTruthy())
		await fireEvent.click(container.querySelector('.chip-x') as HTMLButtonElement)
		await vi.waitFor(() => expect(container.querySelector('.chip')).toBeNull())
		expect(removeNoteTag).toHaveBeenCalledWith('n1', 'a')
		expect(onChanged).toHaveBeenCalled()
	})

	it('只读：无输入框、chip 无移除按钮；空标签则整体不渲染', async () => {
		noteTags.mockResolvedValue([{ id: 'a', title: 'work' }])
		const { container } = render(NoteTags, { props: { noteId: 'n1', readOnly: true } })
		await vi.waitFor(() => expect(container.querySelector('.chip')).toBeTruthy())
		expect(container.querySelector('input.add')).toBeNull()
		expect(container.querySelector('.chip-x')).toBeNull()

		noteTags.mockResolvedValue([])
		const { container: c2 } = render(NoteTags, { props: { noteId: 'n2', readOnly: true } })
		await tick()
		expect(c2.querySelector('.note-tags')).toBeNull()
	})
})
