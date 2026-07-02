// ChatWidget：消息前端持有；发送 → onSend(input, messages 含新 user 消息) → 回复追加（spec §9.2）。
import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/svelte'
import { tick } from 'svelte'
import ChatWidget from './ChatWidget.svelte'
import type { ChatMessage } from './api'

describe('ChatWidget', () => {
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
})
