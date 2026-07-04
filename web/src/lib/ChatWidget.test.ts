// ChatWidget：消息前端持有；发送 → onSend(input, messages 含新 user 消息) → 回复追加（spec §9.2）。
// + 多会话 + localStorage 持久化（storageKey）。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, fireEvent } from '@testing-library/svelte'
import { tick } from 'svelte'
import ChatWidget from './ChatWidget.svelte'
import type { ChatMessage } from './api'

async function typeAndSend(container: HTMLElement, text: string) {
	const ta = container.querySelector('textarea') as HTMLTextAreaElement
	await fireEvent.input(ta, { target: { value: text } })
	await fireEvent.keyDown(ta, { key: 'Enter' })
}

describe('ChatWidget', () => {
	beforeEach(() => localStorage.clear())

	it('发送后追加 user 消息，回复以 markdown 渲染追加', async () => {
		let seen: ChatMessage[] = []
		const onSend = vi.fn(async (_input: string, messages: ChatMessage[]) => {
			seen = messages
			return '**hi** back'
		})
		const { container } = render(ChatWidget, { props: { onSend } })
		const ta = container.querySelector('textarea') as HTMLTextAreaElement
		await fireEvent.input(ta, { target: { value: '你好' } })
		await fireEvent.keyDown(ta, { key: 'Enter' })
		await tick()
		await vi.waitFor(() => {
			expect(container.querySelectorAll('.msg').length).toBe(2)
		})
		expect(onSend).toHaveBeenCalledWith('你好', expect.any(Array))
		expect(seen.at(-1)).toEqual({ role: 'user', content: '你好' })
		expect(container.querySelector('.msg.assistant strong')?.textContent).toBe('hi')
	})

	it('回复为 null（失败已被上层兜住）→ 不追加 assistant 消息', async () => {
		const onSend = vi.fn(async () => null)
		const { container } = render(ChatWidget, { props: { onSend } })
		const ta = container.querySelector('textarea') as HTMLTextAreaElement
		await fireEvent.input(ta, { target: { value: 'x' } })
		await fireEvent.keyDown(ta, { key: 'Enter' })
		await vi.waitFor(() => {
			expect(onSend).toHaveBeenCalled()
		})
		await tick()
		expect(container.querySelectorAll('.msg').length).toBe(1)
	})

	it('空输入不发送', async () => {
		const onSend = vi.fn(async () => null)
		const { container } = render(ChatWidget, { props: { onSend } })
		const ta = container.querySelector('textarea') as HTMLTextAreaElement
		await fireEvent.keyDown(ta, { key: 'Enter' })
		expect(onSend).not.toHaveBeenCalled()
	})

	it('storageKey：消息落 localStorage，重开同键会话续上', async () => {
		const onSend = vi.fn(async () => '收到')
		const first = render(ChatWidget, { props: { onSend, storageKey: 'p/panel' } })
		await typeAndSend(first.container, '记住我')
		await vi.waitFor(() => expect(onSend).toHaveBeenCalled())
		await tick()
		first.unmount()

		// 重新挂载（模拟关闭 dock 再打开）→ 上一条对话仍在
		const again = render(ChatWidget, { props: { onSend, storageKey: 'p/panel' } })
		await tick()
		expect(again.container.textContent).toContain('记住我')
		expect(again.container.textContent).toContain('收到')
	})

	it('新建会话清空当前视图；不同 storageKey 互相隔离', async () => {
		const onSend = vi.fn(async () => 'r')
		const { container, getByTitle } = render(ChatWidget, { props: { onSend, storageKey: 'p/a' } })
		await typeAndSend(container, '第一条')
		await vi.waitFor(() => expect(onSend).toHaveBeenCalled())
		await tick()
		expect(container.textContent).toContain('第一条')

		// 点「新建会话」→ 消息区清空（新空会话）
		await fireEvent.click(getByTitle('New session'))
		await tick()
		expect(container.querySelectorAll('.msg').length).toBe(0)

		// 另一个 storageKey 的面板不受影响（隔离）
		const other = render(ChatWidget, { props: { onSend, storageKey: 'p/b' } })
		await tick()
		expect(other.container.textContent).not.toContain('第一条')
	})
})
