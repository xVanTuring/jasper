// SchemaForm：字段类型 → 控件映射、默认值预填、双向绑定、错误与 secret 占位。
import { describe, it, expect, beforeEach } from 'vitest'
import { render, fireEvent } from '@testing-library/svelte'
import SchemaForm from './SchemaForm.svelte'
import { setLocale } from './i18n.svelte'
import { defaultValues, type Schema } from './schema'

const schema: Schema = {
	url: { type: 'string', label: '地址', required: true, placeholder: 'https://…' },
	pass: { type: 'secret', label: '密码' },
	desc: { type: 'multiline' },
	port: { type: 'number', default: 8080 },
	tls: { type: 'bool', label: '启用 TLS', default: true },
	mode: { type: 'select', options: ['a', 'b'] },
}

beforeEach(() => setLocale('zh'))

describe('SchemaForm', () => {
	it('renders the right control per field type with defaults prefilled', () => {
		const { container } = render(SchemaForm, { props: { schema, values: defaultValues(schema) } })
		expect(container.querySelector('input[type="text"]')).toBeTruthy()
		expect(container.querySelector('input[type="password"]')).toBeTruthy()
		expect(container.querySelector('textarea')).toBeTruthy()
		const num = container.querySelector('input[type="number"]') as HTMLInputElement
		expect(num.value).toBe('8080')
		const check = container.querySelector('input[type="checkbox"]') as HTMLInputElement
		expect(check.checked).toBe(true)
		expect(container.querySelector('select')).toBeTruthy()
		expect((container.querySelector('input[type="text"]') as HTMLInputElement).placeholder).toBe('https://…')
	})

	it('updates bound values on input', async () => {
		// 组件对 values 做属性级修改（values[key] = …）→ 传入同一引用即可观察到
		const values: Record<string, unknown> = defaultValues(schema)
		const { container } = render(SchemaForm, { props: { schema, values } })
		const url = container.querySelector('input[type="text"]') as HTMLInputElement
		await fireEvent.input(url, { target: { value: 'https://x/' } })
		expect(values.url).toBe('https://x/')
		const check = container.querySelector('input[type="checkbox"]') as HTMLInputElement
		await fireEvent.change(check, { target: { checked: false } })
		expect(values.tls).toBe(false)
	})

	it('shows error text via i18n and secret-set placeholder', () => {
		const { container } = render(SchemaForm, {
			props: {
				schema,
				values: {},
				errors: { url: 'required' },
				secretSet: { pass: true },
			},
		})
		expect(container.textContent).toContain('必填')
		const pass = container.querySelector('input[type="password"]') as HTMLInputElement
		expect(pass.placeholder).toBe('已设置，留空保持不变')
	})
})
