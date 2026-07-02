import { describe, it, expect } from 'vitest'
import { defaultValues, validate, type Schema } from './schema'

const schema: Schema = {
	url: { type: 'string', required: true, placeholder: 'https://…' },
	user: { type: 'string' },
	pass: { type: 'secret' },
	note: { type: 'multiline' },
	port: { type: 'number', default: 8080 },
	tls: { type: 'bool', default: true },
	mode: { type: 'select', options: ['a', 'b'] },
}

describe('defaultValues', () => {
	it('applies explicit defaults and bool=false fallback', () => {
		const d = defaultValues({ ...schema, plain: { type: 'bool' } })
		expect(d).toEqual({ port: 8080, tls: true, plain: false })
	})
})

describe('validate', () => {
	it('accepts a full valid form and cleans number strings', () => {
		const { ok, cleaned, errors } = validate(schema, {
			url: 'https://x/',
			user: 'u',
			pass: 'p',
			port: ' 9090 ', // 输入框给字符串
			tls: true,
			mode: 'a',
		})
		expect(errors).toEqual({})
		expect(ok).toBe(true)
		expect(cleaned.port).toBe(9090)
		expect(cleaned.mode).toBe('a')
	})

	it('flags missing required string', () => {
		const { ok, errors } = validate(schema, { url: '   ' })
		expect(ok).toBe(false)
		expect(errors.url).toBe('required')
	})

	it('flags non-numeric number and invalid select option', () => {
		const r = validate(schema, { url: 'x', port: 'abc', mode: 'z' })
		expect(r.errors.port).toBe('invalidNumber')
		expect(r.errors.mode).toBe('invalidOption')
	})

	it('omits empty optional fields from cleaned values', () => {
		const { cleaned } = validate(schema, { url: 'x', user: '', pass: '' })
		expect('user' in cleaned).toBe(false)
		expect('pass' in cleaned).toBe(false)
		// bool 恒有值（checkbox 没有「缺省」态）
		expect(cleaned.tls).toBe(false)
	})
})
