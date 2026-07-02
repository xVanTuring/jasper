<script lang="ts">
	// 插件写提案确认框（spec 0.3 §6.5/§9.5）：逐条弹出队首提案的 diff，
	// 同意 → 走普通 PUT/POST /api/notes*（before-save 钩子照常）；拒绝 → 出队即弃。
	import { fade, scale } from 'svelte/transition'
	import { cubicOut } from 'svelte/easing'
	import { api, type PendingWrite } from './api'
	import { currentPendingWrite, pendingWriteQueue, shiftPendingWrite } from './pendingWrites.svelte'
	import { pluginList } from './plugins.svelte'
	import { diffLines } from './diff'
	import { t } from './i18n.svelte'
	import Button from './Button.svelte'

	let {
		onApplied,
	}: {
		/** 提案已落盘后回调（App 刷新列表/详情/树） */
		onApplied?: (write: PendingWrite) => void
	} = $props()

	let write = $derived(currentPendingWrite())
	let applying = $state(false)
	let error = $state('')

	let pluginName = $derived.by(() => {
		const id = write?.plugin_id ?? ''
		return pluginList().find((p) => p.id === id)?.name || id
	})

	let titleChanged = $derived(write?.original != null && write.original.title !== write.note.title)
	let diff = $derived.by(() => {
		if (!write || write.action === 'create') return []
		return diffLines(write.original?.body ?? '', write.note.body)
	})

	async function approve() {
		if (!write || applying) return
		applying = true
		error = ''
		try {
			if (write.action === 'update') {
				await api.updateNote(write.note.id, { title: write.note.title, body: write.note.body })
			} else {
				await api.createNote({
					parent_id: write.note.parent_id,
					title: write.note.title,
					body: write.note.body,
				})
			}
			const applied = write
			shiftPendingWrite()
			onApplied?.(applied)
		} catch (e) {
			error = t('pw.applyFailed', { msg: e instanceof Error ? e.message : `${e}` })
		} finally {
			applying = false
		}
	}

	function reject() {
		error = ''
		shiftPendingWrite()
	}
</script>

{#if write}
	<div class="overlay" role="presentation" transition:fade={{ duration: 120 }}>
		<div class="card" transition:scale={{ duration: 170, start: 0.95, opacity: 0, easing: cubicOut }}>
			<h3>
				{write.action === 'update'
					? t('pw.titleUpdate', { name: pluginName })
					: t('pw.titleCreate', { name: pluginName })}
			</h3>
			<p class="meta">
				{t('pw.note', { title: write.note.title || t('common.untitled') })}
				{#if write.action === 'create'}
					<span class="dim">{t('pw.folder', { id: write.note.parent_id })}</span>
				{/if}
			</p>
			{#if titleChanged && write.original}
				<p class="title-change"><del>{write.original.title}</del> → <ins>{write.note.title}</ins></p>
			{/if}

			<div class="diff-title">{t('pw.diffTitle')}</div>
			<div class="diff">
				{#if write.action === 'create'}
					{#each write.note.body.split('\n') as line, i (i)}
						<div class="line add">{line || ' '}</div>
					{/each}
				{:else}
					{#each diff as l, i (i)}
						<div class="line {l.type}">{l.text || ' '}</div>
					{/each}
				{/if}
			</div>

			{#if error}<p class="error">{error}</p>{/if}
			<div class="actions">
				{#if pendingWriteQueue().length > 1}
					<span class="queue">{t('pw.queue', { n: pendingWriteQueue().length - 1 })}</span>
				{/if}
				<Button label={t('pw.reject')} onclick={reject} disabled={applying} />
				<Button variant="primary" label={t('pw.approve')} onclick={approve} disabled={applying} />
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		z-index: 130; /* 高于 PluginPanel(100)/Consent(120)：写确认永远最上层 */
		background: var(--overlay);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.card {
		width: 640px;
		max-width: calc(100vw - 32px);
		max-height: calc(100vh - 80px);
		display: flex;
		flex-direction: column;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 18px 20px;
		box-shadow: var(--shadow-modal);
	}
	h3 {
		margin: 0 0 6px;
		font-size: 15px;
	}
	.meta {
		margin: 0 0 8px;
		font-size: 13px;
		color: var(--text-dim);
	}
	.meta .dim {
		margin-left: 8px;
	}
	.title-change {
		margin: 0 0 8px;
		font-size: 13px;
	}
	.title-change del {
		color: var(--danger);
	}
	.title-change ins {
		color: var(--success);
		text-decoration: none;
	}
	.diff-title {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-dim);
		margin-bottom: 4px;
	}
	.diff {
		flex: 1;
		min-height: 60px;
		overflow: auto;
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 6px 0;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 12px;
		line-height: 1.55;
	}
	.line {
		padding: 0 10px;
		white-space: pre-wrap;
		word-break: break-word;
	}
	.line.add {
		background: color-mix(in srgb, var(--success) 14%, transparent);
	}
	.line.del {
		background: var(--danger-soft-weak);
		text-decoration: line-through;
		color: var(--text-dim);
	}
	.error {
		margin: 8px 0 0;
		color: var(--danger);
		font-size: 13px;
	}
	.actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 14px;
	}
	.queue {
		margin-right: auto;
		font-size: 12px;
		color: var(--text-dim);
	}
</style>
