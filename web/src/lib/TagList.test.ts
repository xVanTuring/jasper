// TagList：侧栏标签浏览区（只读）。
import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/svelte'
import TagList from './TagList.svelte'
import type { TagInfo } from './api'

const tags: TagInfo[] = [
	{ id: 'a'.repeat(32), title: 'idea', note_count: 3 },
	{ id: 'b'.repeat(32), title: 'work', note_count: 0 },
]

describe('TagList', () => {
	it('无标签时整区不渲染', () => {
		const { container } = render(TagList, { props: { tags: [], selectedId: null, onSelect: vi.fn() } })
		expect(container.textContent?.trim()).toBe('')
	})

	it('渲染标签名与篇数（篇数 0 不显示徽标）', () => {
		const { getByText, container } = render(TagList, {
			props: { tags, selectedId: null, onSelect: vi.fn() },
		})
		expect(getByText('idea')).toBeTruthy()
		expect(getByText('work')).toBeTruthy()
		// 只有 idea 有篇数徽标 3；work 为 0 不显示
		const counts = Array.from(container.querySelectorAll('.count')).map((e) => e.textContent)
		expect(counts).toEqual(['3'])
	})

	it('点击标签 → onSelect(id, title)', async () => {
		const onSelect = vi.fn()
		const { getByText } = render(TagList, { props: { tags, selectedId: null, onSelect } })
		await fireEvent.click(getByText('idea'))
		expect(onSelect).toHaveBeenCalledWith('a'.repeat(32), 'idea')
	})

	it('选中项标记 active', () => {
		const { container } = render(TagList, {
			props: { tags, selectedId: 'a'.repeat(32), onSelect: vi.fn() },
		})
		const active = container.querySelector('.row.active')
		expect(active?.textContent).toContain('idea')
	})
})
