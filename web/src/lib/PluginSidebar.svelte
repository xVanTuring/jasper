<script lang="ts">
	// 插件侧边栏面板（右侧 dock，spec §3.5/§9.4）：
	// - 静态模式（无 view）：直接渲染声明的 widget，交互经 sidebar.command；
	// - 动态模式（有 view）：挂载时经 POST /api/plugins/{id}/ui/{view} 取 UiNode 树渲染，
	//   命令 result 含 ui → 替换整棵树（server-driven UI 循环）。
	// 一切命令调用并入当前笔记 note_id；响应里的 pending_writes 交全局确认队列。
	import { api, type UiNode } from './api'
	import type { SidebarEntry } from './plugins.svelte'
	import { enqueuePendingWrites } from './pendingWrites.svelte'
	import { currentSelectionText } from './selection.svelte'
	import { storageKeyFor } from './chatSessions'
	import { t, resolveText } from './i18n.svelte'
	import Button from './Button.svelte'
	import Icon from './Icon.svelte'
	import ChatWidget from './ChatWidget.svelte'
	import UiWidget from './UiWidget.svelte'
	import type { ChatMessage } from './api'

	let {
		entry,
		noteId,
		onClose,
		onNotesChanged,
	}: {
		entry: SidebarEntry
		noteId: string | null
		onClose: () => void
		/** 命令执行后回调（免确认直写可能已改库，App 刷新列表） */
		onNotesChanged?: () => void
	} = $props()

	let tree = $state<UiNode | null>(null)
	let loading = $state(false)
	let error = $state('')

	let view = $derived(entry.contribution.view)

	$effect(() => {
		if (view) void loadTree(view)
	})

	async function loadTree(v: string) {
		loading = true
		error = ''
		try {
			const r = await api.pluginUi(entry.pluginId, v, { note_id: noteId })
			enqueuePendingWrites(r.pending_writes)
			tree = r.ui
		} catch (e) {
			error = t('plugins.sidebar.loadFailed', { msg: e instanceof Error ? e.message : `${e}` })
		} finally {
			loading = false
		}
	}

	/** 统一命令出口：并入 note_id + 当前笔记选区、提案入队、result.ui 换树；失败挂到面板错误条并返回 null。 */
	async function runCommand(command: string, args: Record<string, unknown>): Promise<Record<string, unknown> | null> {
		error = ''
		try {
			// 当前在笔记内容区选中的文字（spec §9.2）：非空才并入，供「优化选中段落」等场景
			const selText = currentSelectionText()
			const selection = selText ? { text: selText } : undefined
			const r = await api.runPluginCommand(entry.pluginId, command, { ...args, note_id: noteId, selection })
			enqueuePendingWrites(r.pending_writes)
			const ui = r.result.ui
			if (ui && typeof ui === 'object' && typeof (ui as UiNode).type === 'string') {
				tree = ui as UiNode
			}
			onNotesChanged?.()
			return r.result
		} catch (e) {
			error = e instanceof Error ? e.message : `${e}`
			return null
		}
	}

	// 静态 chat：消息前端持有，发送契约 args = {messages, input, note_id}（spec §9.2）
	async function chatSend(input: string, messages: ChatMessage[]): Promise<string | null> {
		const command = entry.contribution.command
		if (!command) return null
		const result = await runCommand(command, { messages, input })
		const reply = result?.reply
		// reply 可为字符串或 locale map（多语言回复），前端按当前语言挑（spec §9.2）
		return reply == null ? null : resolveText(reply)
	}

	// 静态非 chat widget：合成单节点树（command 由贡献声明提供）
	let staticNode = $derived.by((): UiNode => {
		const c = entry.contribution
		return { type: c.widget, props: c.command ? { command: c.command } : {} }
	})
</script>

<div class="panel">
	<div class="panel-title">
		<Icon name={entry.contribution.icon || 'plug'} size={14} />
		<span class="title">{entry.contribution.title}</span>
		<Button variant="ghost" iconOnly icon="close" label={t('common.close')} onclick={onClose} />
	</div>
	{#if error}
		<div class="error">{error}</div>
	{/if}
	<div class="body" class:chat-body={!view && entry.contribution.widget === 'chat'}>
		{#if view}
			{#if loading && !tree}
				<p class="dim">{t('common.loading')}</p>
			{:else if tree}
				<UiWidget node={tree} onCommand={runCommand} />
			{/if}
		{:else if entry.contribution.widget === 'chat'}
			<ChatWidget onSend={chatSend} storageKey={storageKeyFor(entry.pluginId, entry.contribution.id)} />
		{:else}
			<UiWidget node={staticNode} onCommand={runCommand} />
		{/if}
	</div>
</div>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}
	.panel-title {
		position: sticky;
		top: 0;
		z-index: 10;
		height: 38px;
		flex: 0 0 auto;
		display: flex;
		align-items: center;
		gap: 6px;
		box-sizing: border-box;
		background: var(--bg-side);
		padding: 0 6px 0 12px;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-dim);
		border-bottom: 1px solid var(--border);
	}
	.panel-title .title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.error {
		flex: 0 0 auto;
		padding: 6px 12px;
		font-size: 12px;
		color: var(--danger);
		border-bottom: 1px solid var(--border);
	}
	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 10px 12px;
	}
	.body.chat-body {
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
	.dim {
		color: var(--text-dim);
		font-size: 13px;
	}
</style>
