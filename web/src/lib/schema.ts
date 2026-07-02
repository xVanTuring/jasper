// 插件设置 / 存储配置表单的字段词汇（plugin-spec §10）——纯 TS，无 DOM。
// SchemaForm.svelte 负责渲染；这里负责默认值与校验，便于 Vitest 直接覆盖。

export interface FieldDef {
	type: 'string' | 'multiline' | 'secret' | 'bool' | 'number' | 'select'
	label?: string
	default?: unknown
	description?: string
	options?: string[]
	required?: boolean
	placeholder?: string
}

export type Schema = Record<string, FieldDef>
export type FieldValues = Record<string, unknown>
export type FieldError = 'required' | 'invalidNumber' | 'invalidOption'

/** 按 schema 生成初始表单值：显式 default 优先，bool 缺省 false，其余留空。 */
export function defaultValues(schema: Schema): FieldValues {
	const out: FieldValues = {}
	for (const [k, f] of Object.entries(schema)) {
		if (f.default !== undefined) out[k] = f.default
		else if (f.type === 'bool') out[k] = false
	}
	return out
}

/**
 * 校验并清洗表单值：number 字段接受数字字符串并转 number；
 * 空的非必填字段从结果中省略（服务端按缺省处理）。
 * 注意：secret 空串的「保持不变」语义由调用方决定（设置页省略该键即可）。
 */
export function validate(
	schema: Schema,
	values: FieldValues,
): { ok: boolean; cleaned: FieldValues; errors: Partial<Record<string, FieldError>> } {
	const cleaned: FieldValues = {}
	const errors: Partial<Record<string, FieldError>> = {}
	for (const [k, f] of Object.entries(schema)) {
		const v = values[k]
		switch (f.type) {
			case 'bool':
				cleaned[k] = Boolean(v)
				break
			case 'number': {
				if (v === undefined || v === null || v === '') {
					if (f.required) errors[k] = 'required'
					break
				}
				const n = typeof v === 'number' ? v : Number(String(v).trim())
				if (Number.isNaN(n)) errors[k] = 'invalidNumber'
				else cleaned[k] = n
				break
			}
			case 'select': {
				const s = typeof v === 'string' ? v : ''
				if (!s) {
					if (f.required) errors[k] = 'required'
					break
				}
				if (!(f.options ?? []).includes(s)) errors[k] = 'invalidOption'
				else cleaned[k] = s
				break
			}
			// string | multiline | secret
			default: {
				const s = typeof v === 'string' ? v : v == null ? '' : String(v)
				if (f.required && !s.trim()) {
					errors[k] = 'required'
					break
				}
				if (s !== '') cleaned[k] = s
				break
			}
		}
	}
	return { ok: Object.keys(errors).length === 0, cleaned, errors }
}
