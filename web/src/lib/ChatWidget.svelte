<script lang="ts">
	// chat widget（spec §9.2）：消息列表由前端持有（会话内存，不落盘）。
	// 发送 → onSend(input, messages)（含刚入列的 user 消息）→ 返回 assistant 回复文本则追加。
	import type { ChatMessage } from './api'
	import { renderMarkdown } from './render'
	import { t } from './i18n.svelte'
	import Button from './Button.svelte'
	import Icon from './Icon.svelte'

	let {
		placeholder = '',
		onSend,
	}: {
		placeholder?: string
		/** 返回 assistant 回复（null = 无回复；异常由调用方兜住并返回 null） */
		onSend: (input: string, messages: ChatMessage[]) => Promise<string | null>
	} = $props()

	let messages = $state<ChatMessage[]>([])
	let input = $state('')
	let sending = $state(false)
	let listEl: HTMLElement | null = $state(null)

	function scrollToEnd() {
		requestAnimationFrame(() => listEl?.scrollTo({ top: listEl.scrollHeight }))
	}

	async function send() {
		const text = input.trim()
		if (!text || sending) return
		input = ''
		messages = [...messages, { role: 'user', content: text }]
		scrollToEnd()
		sending = true
		try {
			const reply = await onSend(text, messages)
			if (typeof reply === 'string' && reply) {
				messages = [...messages, { role: 'assistant', content: reply }]
				scrollToEnd()
			}
		} finally {
			sending = false
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault()
			void send()
		}
	}
</script>

<div class="chat">
	<div class="msgs" bind:this={listEl}>
		{#if !messages.length}
			<p class="empty">{t('chat.empty')}</p>
		{/if}
		{#each messages as m, i (i)}
			{#if m.role !== 'system'}
				<div class="msg {m.role}">
					{#if m.role === 'assistant'}
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- renderMarkdown 已过 DOMPurify -->
						<div class="bubble md">{@html renderMarkdown(m.content)}</div>
					{:else}
						<div class="bubble">{m.content}</div>
					{/if}
				</div>
			{/if}
		{/each}
		{#if sending}
			<div class="msg assistant">
				<div class="bubble busy"><Icon name="bot" size={14} /> {t('chat.busy')}</div>
			</div>
		{/if}
	</div>
	<div class="composer">
		<textarea
			rows="2"
			placeholder={placeholder || t('chat.placeholder')}
			bind:value={input}
			onkeydown={onKeydown}
			disabled={sending}
		></textarea>
		<Button icon="send" label={t('chat.send')} iconOnly onclick={() => void send()} disabled={sending || !input.trim()} />
	</div>
</div>

<style>
	.chat {
		display: flex;
		flex-direction: column;
		min-height: 0;
		flex: 1;
	}
	.msgs {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 8px 2px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.empty {
		color: var(--text-dim);
		font-size: 13px;
		text-align: center;
		margin-top: 24px;
	}
	.msg {
		display: flex;
	}
	.msg.user {
		justify-content: flex-end;
	}
	.bubble {
		max-width: 88%;
		padding: 6px 10px;
		border-radius: 10px;
		font-size: 13px;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
	}
	.msg.user .bubble {
		background: var(--accent-soft);
	}
	.msg.assistant .bubble {
		background: var(--hover);
		white-space: normal;
	}
	.bubble.busy {
		color: var(--text-dim);
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.bubble.md :global(p:first-child) {
		margin-top: 0;
	}
	.bubble.md :global(p:last-child) {
		margin-bottom: 0;
	}
	.bubble.md :global(pre) {
		overflow-x: auto;
	}
	.composer {
		display: flex;
		gap: 6px;
		align-items: flex-end;
		padding-top: 8px;
		border-top: 1px solid var(--border);
	}
	.composer textarea {
		flex: 1;
		resize: none;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		color: var(--text);
		font: inherit;
		font-size: 13px;
	}
	.composer textarea:focus {
		outline: none;
		border-color: var(--accent);
	}
</style>
