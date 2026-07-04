<script lang="ts">
	// chat widget（spec §9.2）：消息由前端持有；storageKey 存在时**多会话 + localStorage 持久化**
	// （关闭/重开、切会话都不丢），否则退化为纯内存单会话（旧行为/测试兜底）。
	// 发送 → onSend(input, messages)（含刚入列的 user 消息）→ 返回 assistant 回复文本则追加。
	// 当前笔记内容区的选区由 selection.svelte 捕获、命令层（PluginSidebar）并入 args；这里只做提示 chip。
	import type { ChatMessage } from './api'
	import { renderMarkdown } from './render'
	import { t } from './i18n.svelte'
	import Button from './Button.svelte'
	import Icon from './Icon.svelte'
	import { currentSelectionText, clearSelection } from './selection.svelte'
	import {
		loadStore,
		persistStore,
		appendMessage,
		startSession,
		switchSession,
		removeSession,
		renameSession,
		activeSession,
		sessionTitle,
		type SessionStore,
	} from './chatSessions'

	let {
		placeholder = '',
		onSend,
		storageKey = '',
	}: {
		placeholder?: string
		/** 返回 assistant 回复（null = 无回复；异常由调用方兜住并返回 null） */
		onSend: (input: string, messages: ChatMessage[]) => Promise<string | null>
		/** 非空 = 多会话持久化（按此键隔离存 localStorage）；空 = 纯内存单会话 */
		storageKey?: string
	} = $props()

	function genId(): string {
		return typeof crypto !== 'undefined' && crypto.randomUUID
			? crypto.randomUUID()
			: `s${Date.now()}-${Math.random().toString(36).slice(2)}`
	}
	const now = () => Date.now()

	let store = $state<SessionStore>(loadStore(storageKey, genId, now))
	// 变更即持久化（storageKey 为空时不落盘）
	$effect(() => {
		if (storageKey) persistStore(storageKey, store)
	})

	let active = $derived(activeSession(store))
	let messages = $derived(active.messages)
	let selText = $derived(currentSelectionText())

	let input = $state('')
	let sending = $state(false)
	let listEl: HTMLElement | null = $state(null)
	let menuOpen = $state(false)
	let renaming = $state(false)
	let renameText = $state('')

	function scrollToEnd() {
		requestAnimationFrame(() => listEl?.scrollTo({ top: listEl.scrollHeight }))
	}

	async function send() {
		const text = input.trim()
		if (!text || sending) return
		input = ''
		store = appendMessage(store, { role: 'user', content: text }, now())
		scrollToEnd()
		sending = true
		try {
			const reply = await onSend(text, activeSession(store).messages)
			if (typeof reply === 'string' && reply) {
				store = appendMessage(store, { role: 'assistant', content: reply }, now())
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

	// ---------- 会话操作 ----------
	function newSession() {
		store = startSession(store, genId, now())
		menuOpen = false
		input = ''
		scrollToEnd()
	}
	function switchTo(id: string) {
		store = switchSession(store, id)
		menuOpen = false
		scrollToEnd()
	}
	function del(id: string) {
		store = removeSession(store, id, genId, now())
	}
	function startRename() {
		renameText = sessionTitle(active)
		renaming = true
		menuOpen = false
	}
	function commitRename() {
		store = renameSession(store, active.id, renameText.trim())
		renaming = false
	}
	function renameKey(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault()
			commitRename()
		} else if (e.key === 'Escape') {
			renaming = false
		}
	}
</script>

<div class="chat">
	<div class="sbar">
		{#if renaming}
			<!-- svelte-ignore a11y_autofocus -->
			<input
				class="rename"
				bind:value={renameText}
				onblur={commitRename}
				onkeydown={renameKey}
				autofocus
				placeholder={t('chat.newChat')}
			/>
		{:else}
			<button class="stitle" onclick={() => (menuOpen = !menuOpen)} title={t('chat.sessions')} class:on={menuOpen}>
				<Icon name="chat" size={13} />
				<span class="ttl">{sessionTitle(active) || t('chat.newChat')}</span>
			</button>
			<span class="grow"></span>
			<Button variant="ghost" iconOnly icon="edit" label={t('chat.renameSession')} onclick={startRename} />
			<Button variant="ghost" iconOnly icon="plus" label={t('chat.newSession')} onclick={newSession} />
		{/if}
	</div>

	{#if menuOpen}
		<div class="smenu">
			{#each store.sessions as s (s.id)}
				<div class="srow" class:on={s.id === store.activeId}>
					<button class="spick" onclick={() => switchTo(s.id)}>{sessionTitle(s) || t('chat.newChat')}</button>
					<button class="sdel" onclick={() => del(s.id)} title={t('chat.deleteSession')}>
						<Icon name="trash" size={12} />
					</button>
				</div>
			{/each}
		</div>
	{/if}

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
		{#if selText}
			<div class="sel-chip">
				<Icon name="edit" size={12} />
				<span>{t('chat.selectionAttached', { n: selText.length })}</span>
				<button class="sel-x" onclick={() => clearSelection()} title={t('chat.selectionClear')}>
					<Icon name="close" size={11} />
				</button>
			</div>
		{/if}
		<div class="row">
			<textarea
				rows="2"
				placeholder={placeholder || t('chat.placeholder')}
				bind:value={input}
				onkeydown={onKeydown}
				disabled={sending}
			></textarea>
			<Button
				icon="send"
				label={t('chat.send')}
				iconOnly
				onclick={() => void send()}
				disabled={sending || !input.trim()}
			/>
		</div>
	</div>
</div>

<style>
	.chat {
		display: flex;
		flex-direction: column;
		min-height: 0;
		flex: 1;
		position: relative;
	}
	.sbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 0 6px;
		border-bottom: 1px solid var(--border);
		margin-bottom: 4px;
	}
	.grow {
		flex: 1;
	}
	.stitle {
		display: flex;
		align-items: center;
		gap: 6px;
		max-width: 62%;
		padding: 4px 8px;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		font-size: 12px;
		border-radius: 6px;
		cursor: pointer;
	}
	.stitle:hover,
	.stitle.on {
		background: var(--hover);
	}
	.stitle .ttl {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.rename {
		flex: 1;
		padding: 4px 8px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font: inherit;
		font-size: 12px;
	}
	.smenu {
		position: absolute;
		top: 30px;
		left: 0;
		z-index: 5;
		width: 78%;
		max-height: 240px;
		overflow-y: auto;
		background: var(--bg-bar);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: var(--shadow-modal);
		padding: 4px;
	}
	.srow {
		display: flex;
		align-items: center;
		gap: 2px;
		border-radius: 6px;
	}
	.srow.on {
		background: var(--accent-soft);
	}
	.spick {
		flex: 1;
		min-width: 0;
		text-align: left;
		padding: 6px 8px;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		font-size: 12px;
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		border-radius: 6px;
	}
	.srow:hover {
		background: var(--hover);
	}
	.sdel {
		display: flex;
		padding: 6px;
		border: none;
		background: none;
		color: var(--text-dim);
		cursor: pointer;
		border-radius: 6px;
	}
	.sdel:hover {
		color: var(--danger);
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
		padding-top: 8px;
		border-top: 1px solid var(--border);
	}
	.sel-chip {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 6px;
		padding: 4px 8px;
		font-size: 12px;
		color: var(--text-dim);
		background: var(--accent-soft);
		border-radius: 6px;
	}
	.sel-chip span {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.sel-x {
		display: flex;
		padding: 2px;
		border: none;
		background: none;
		color: var(--text-dim);
		cursor: pointer;
		border-radius: 4px;
	}
	.sel-x:hover {
		color: var(--text);
	}
	.row {
		display: flex;
		gap: 6px;
		align-items: flex-end;
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
