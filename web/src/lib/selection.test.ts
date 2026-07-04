// selection.svelte.ts：捕获笔记内容区（[data-ai-selectable]）里的文字选区。
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { installSelectionCapture, currentSelectionText, clearSelection } from './selection.svelte'

// 造一个「选区」替身：anchorNode + toString
function fakeSelection(anchorNode: Node | null, text: string) {
	return { rangeCount: anchorNode ? 1 : 0, anchorNode, toString: () => text } as unknown as Selection
}

function fireSelection(sel: Selection) {
	document.getSelection = vi.fn(() => sel)
	document.dispatchEvent(new Event('selectionchange'))
}

describe('selection capture', () => {
	beforeEach(() => {
		document.body.innerHTML = `
			<div data-ai-selectable><p id="note">note text here</p></div>
			<div id="chat"><textarea id="ta"></textarea></div>`
		installSelectionCapture() // 幂等：只装一次
		clearSelection()
	})

	it('记住笔记内容区里选中的文字', () => {
		const note = document.getElementById('note')!.firstChild! // text node
		fireSelection(fakeSelection(note, 'selected paragraph'))
		expect(currentSelectionText()).toBe('selected paragraph')
	})

	it('选区在聊天区（容器外）→ 保持已存的笔记选区不动', () => {
		const note = document.getElementById('note')!.firstChild!
		fireSelection(fakeSelection(note, 'kept note text'))
		// 点进聊天输入框：selectionchange 锚在 textarea → 不该清掉
		const ta = document.getElementById('ta')!
		fireSelection(fakeSelection(ta, ''))
		expect(currentSelectionText()).toBe('kept note text')
	})

	it('笔记内容区里折叠选区（空）→ 清空', () => {
		const note = document.getElementById('note')!.firstChild!
		fireSelection(fakeSelection(note, 'x'))
		expect(currentSelectionText()).toBe('x')
		fireSelection(fakeSelection(note, '')) // 用户在笔记里点了一下，取消选择
		expect(currentSelectionText()).toBe('')
	})

	it('clearSelection 清空', () => {
		const note = document.getElementById('note')!.firstChild!
		fireSelection(fakeSelection(note, 'y'))
		clearSelection()
		expect(currentSelectionText()).toBe('')
	})
})
