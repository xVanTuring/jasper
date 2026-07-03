<script lang="ts">
	// server-driven UI 渲染器（spec §9.2/§9.3）：一棵 UiNode {type, props, children}。
	// 只渲染已知 widget；未知 type 连同其 children 一起安全忽略。
	// children 仅作纵向堆叠。节点交互统一走 onCommand（args 形状按 §9.2 契约，note_id 由上层并入）。
	import type { ChatMessage, UiNode } from './api'
	import { renderMarkdown } from './render'
	import { defaultValues, validate, type FieldError, type FieldValues, type Schema } from './schema'
	import SchemaForm from './SchemaForm.svelte'
	import Button from './Button.svelte'
	import Icon from './Icon.svelte'
	import ChatWidget from './ChatWidget.svelte'
	import Self from './UiWidget.svelte'
	import { t, resolveText } from './i18n.svelte'

	let {
		node,
		onCommand,
	}: {
		node: UiNode
		/** 执行贡献命令，返回命令 result（失败由上层兜住并返回 null） */
		onCommand: (command: string, args: Record<string, unknown>) => Promise<Record<string, unknown> | null>
	} = $props()

	const KNOWN = ['chat', 'list', 'tree', 'form', 'markdown', 'button']

	let nodeProps = $derived(node.props ?? {})
	// 纯字符串字段（命令 id / 图标令牌，不本地化）
	let str = (k: string): string => (typeof nodeProps[k] === 'string' ? (nodeProps[k] as string) : '')
	// 可本地化文本字段（string | { [locale]: string }，前端按当前语言挑，spec §9.2）
	let text = (k: string): string => resolveText(nodeProps[k])

	interface ListItem {
		id: string
		title: unknown // string | locale map
		subtitle?: unknown
		icon?: string
	}
	interface TreeNode {
		id: string
		title: unknown // string | locale map
		children?: TreeNode[]
	}
	let listItems = $derived(
		Array.isArray(nodeProps.items) ? (nodeProps.items as ListItem[]).filter((i) => i && typeof i.id === 'string') : [],
	)
	let treeNodes = $derived(
		Array.isArray(nodeProps.nodes) ? (nodeProps.nodes as TreeNode[]).filter((n) => n && typeof n.id === 'string') : [],
	)

	// form widget：fields 用 §10 词汇（同 SchemaForm）；提交前前端校验
	let formSchema = $derived((nodeProps.fields ?? {}) as Schema)
	let formValues = $state<FieldValues>({})
	let formErrors = $state<Partial<Record<string, FieldError>>>({})
	$effect(() => {
		// 树替换后重置表单值（以 props.values 优先，缺省用 schema 默认值）
		formValues = { ...defaultValues(formSchema), ...((nodeProps.values as FieldValues) ?? {}) }
		formErrors = {}
	})

	let running = $state(false)
	async function run(command: string, args: Record<string, unknown>) {
		if (!command || running) return
		running = true
		try {
			await onCommand(command, args)
		} finally {
			running = false
		}
	}

	async function submitForm() {
		const r = validate(formSchema, formValues)
		formErrors = r.errors
		if (!r.ok) return
		await run(str('command'), { values: r.cleaned })
	}

	async function chatSend(input: string, messages: ChatMessage[]): Promise<string | null> {
		const command = str('command')
		if (!command) return null
		const result = await onCommand(command, { messages, input })
		const reply = result?.reply
		// reply 可为字符串或 locale map（多语言回复）；缺省 null（动态模式走 result.ui）
		return reply == null ? null : resolveText(reply)
	}
</script>

{#if KNOWN.includes(node.type)}
	{#if node.type === 'markdown'}
		<!-- eslint-disable-next-line svelte/no-at-html-tags -- renderMarkdown 已过 DOMPurify -->
		<div class="w-md">{@html renderMarkdown(text('source'))}</div>
	{:else if node.type === 'button'}
		<div class="w-btn">
			<Button
				icon={str('icon') || undefined}
				label={text('label')}
				disabled={running}
				onclick={() => void run(str('command'), (nodeProps.args as Record<string, unknown>) ?? {})}
			/>
		</div>
	{:else if node.type === 'form'}
		<form
			class="w-form"
			onsubmit={(e) => {
				e.preventDefault()
				void submitForm()
			}}
		>
			<SchemaForm schema={formSchema} bind:values={formValues} errors={formErrors} />
			<Button type="submit" label={text('submit_label') || t('form.submit')} variant="primary" disabled={running} />
		</form>
	{:else if node.type === 'list'}
		<ul class="w-list">
			{#each listItems as item (item.id)}
				<li>
					{#if str('command')}
						<button class="row" disabled={running} onclick={() => void run(str('command'), { id: item.id })}>
							{#if item.icon}<Icon name={item.icon} size={14} />{/if}
							<span class="title">{resolveText(item.title)}</span>
							{#if item.subtitle}<span class="sub">{resolveText(item.subtitle)}</span>{/if}
						</button>
					{:else}
						<span class="row plain">
							{#if item.icon}<Icon name={item.icon} size={14} />{/if}
							<span class="title">{resolveText(item.title)}</span>
							{#if item.subtitle}<span class="sub">{resolveText(item.subtitle)}</span>{/if}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{:else if node.type === 'tree'}
		{#snippet branch(nodes: TreeNode[])}
			<ul class="w-tree">
				{#each nodes as n (n.id)}
					<li>
						{#if str('command')}
							<button class="row" disabled={running} onclick={() => void run(str('command'), { id: n.id })}>
								{resolveText(n.title)}
							</button>
						{:else}
							<span class="row plain">{resolveText(n.title)}</span>
						{/if}
						{#if n.children?.length}
							{@render branch(n.children)}
						{/if}
					</li>
				{/each}
			</ul>
		{/snippet}
		{@render branch(treeNodes)}
	{:else if node.type === 'chat'}
		<ChatWidget placeholder={text('placeholder')} onSend={chatSend} />
	{/if}

	{#each node.children ?? [] as child, i (i)}
		<Self node={child} {onCommand} />
	{/each}
{/if}

<style>
	.w-md {
		font-size: 13px;
		line-height: 1.6;
	}
	.w-md :global(p:first-child) {
		margin-top: 0;
	}
	.w-btn {
		margin: 6px 0;
	}
	.w-form {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		margin: 6px 0;
	}
	.w-list,
	.w-tree {
		list-style: none;
		margin: 6px 0;
		padding: 0;
	}
	.w-tree .w-tree {
		padding-left: 14px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 6px 8px;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		font-size: 13px;
		text-align: left;
		border-radius: 6px;
	}
	button.row {
		cursor: pointer;
	}
	button.row:hover:not(:disabled) {
		background: var(--hover);
	}
	.row.plain {
		cursor: default;
	}
	.row .title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row .sub {
		color: var(--text-dim);
		font-size: 12px;
	}
</style>
