<script lang="ts">
	// 由字段词汇（schema.ts / plugin-spec §10）驱动的表单渲染器。
	// 用途：设置向导的插件数据源配置、插件设置页，将来也是 form widget 的底座。
	import type { FieldError, FieldValues, Schema } from './schema'
	import { t } from './i18n.svelte'

	let {
		schema,
		values = $bindable(),
		errors = {},
		secretSet = {},
	}: {
		schema: Schema
		values: FieldValues
		/** validate() 的错误表（键 → 错误类型），由调用方在提交时填充 */
		errors?: Partial<Record<string, FieldError>>
		/** secret 字段「已设置」标记（不回显场景）：占位符提示留空保持不变 */
		secretSet?: Record<string, boolean>
	} = $props()

	const errorText = (e: FieldError) =>
		e === 'required' ? t('form.required') : e === 'invalidNumber' ? t('form.invalidNumber') : t('form.invalidOption')
</script>

{#each Object.entries(schema) as [key, field] (key)}
	<label class="sf-field">
		{#if field.type === 'bool'}
			<span class="sf-check">
				<input
					type="checkbox"
					checked={Boolean(values[key])}
					onchange={(e) => (values[key] = e.currentTarget.checked)}
				/>
				<span>{field.label ?? key}</span>
			</span>
		{:else}
			<span class="sf-label">{field.label ?? key}</span>
			{#if field.type === 'select'}
				<select
					value={typeof values[key] === 'string' ? (values[key] as string) : ''}
					onchange={(e) => (values[key] = e.currentTarget.value)}
				>
					{#if !field.required}<option value=""></option>{/if}
					{#each field.options ?? [] as opt (opt)}
						<option value={opt}>{opt}</option>
					{/each}
				</select>
			{:else if field.type === 'multiline'}
				<textarea
					rows="4"
					placeholder={field.placeholder ?? ''}
					value={values[key] == null ? '' : String(values[key])}
					oninput={(e) => (values[key] = e.currentTarget.value)}
				></textarea>
			{:else}
				<input
					type={field.type === 'secret' ? 'password' : field.type === 'number' ? 'number' : 'text'}
					placeholder={field.type === 'secret' && secretSet[key] ? t('form.secretSetPh') : (field.placeholder ?? '')}
					value={values[key] == null ? '' : String(values[key])}
					oninput={(e) => (values[key] = e.currentTarget.value)}
				/>
			{/if}
		{/if}
		{#if field.description}<span class="sf-desc">{field.description}</span>{/if}
		{#if errors[key]}<span class="sf-error">{errorText(errors[key])}</span>{/if}
	</label>
{/each}

<style>
	.sf-field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 12px;
		font-size: 13px;
	}
	.sf-label {
		color: var(--text-dim);
	}
	.sf-check {
		display: flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
	}
	.sf-field input[type='text'],
	.sf-field input[type='password'],
	.sf-field input[type='number'],
	.sf-field select,
	.sf-field textarea {
		width: 100%;
		box-sizing: border-box;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font: inherit;
	}
	.sf-field input:focus,
	.sf-field select:focus,
	.sf-field textarea:focus {
		outline: none;
		border-color: var(--accent);
	}
	.sf-desc {
		color: var(--text-dim);
		font-size: 12px;
	}
	.sf-error {
		color: var(--danger);
		font-size: 12px;
	}
</style>
