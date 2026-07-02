// market.ts 纯逻辑单测：索引解析（双语必填/坏条目跳过/schema 版本）、
// 版本比较（对齐 manifest.rs cmp_version）、兼容判断、双语取词、sha256。
import { describe, expect, it, vi } from 'vitest'
import { cmpVersion, compat, parseIndex, pickText, sha256Hex } from './market'

function entry(over: Record<string, unknown> = {}) {
	return {
		id: 's3-storage',
		name: { zh: 'S3 对象存储', en: 'S3 Object Storage' },
		description: { zh: '描述', en: 'desc' },
		author: 'x',
		repo: 'https://github.com/x/y',
		version: '0.1.0',
		apiVersion: '0.2',
		minHostVersion: '',
		capabilities: ['host:http'],
		download: 'https://example.com/a.jplug',
		sha256: 'a'.repeat(64),
		...over,
	}
}

describe('parseIndex', () => {
	it('接受合法索引', () => {
		const out = parseIndex({ schemaVersion: 1, plugins: [entry()] })
		expect(out).toHaveLength(1)
		expect(out[0].id).toBe('s3-storage')
	})

	it('schemaVersion 不认识 → 抛错', () => {
		expect(() => parseIndex({ schemaVersion: 2, plugins: [] })).toThrow(/schemaVersion/)
		expect(() => parseIndex({})).toThrow(/schemaVersion/)
	})

	it('坏条目跳过而不整体失败（缺 en / 坏 id / 非 https / 坏 sha256）', () => {
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
		const out = parseIndex({
			schemaVersion: 1,
			plugins: [
				entry({ name: { zh: '只有中文' } }),
				entry({ id: 'Bad_Id' }),
				entry({ download: 'http://insecure/a.jplug' }),
				entry({ sha256: 'zz' }),
				entry({ id: 'good-one' }),
			],
		})
		expect(out.map((e) => e.id)).toEqual(['good-one'])
		expect(warn).toHaveBeenCalledTimes(4)
		warn.mockRestore()
	})

	it('sha256 归一为小写', () => {
		const out = parseIndex({ schemaVersion: 1, plugins: [entry({ sha256: 'A'.repeat(64).toLowerCase().toUpperCase() })] })
		expect(out).toHaveLength(0) // 大写不匹配 ^[0-9a-f]{64}$ → 跳过（索引以 CI 产物为准，本就是小写）
	})
})

describe('cmpVersion', () => {
	it('数字段按数值比（1.2.0 < 1.10.0）', () => {
		expect(cmpVersion('1.2.0', '1.10.0')).toBeLessThan(0)
		expect(cmpVersion('1.10.0', '1.2.0')).toBeGreaterThan(0)
	})
	it('相等与缺段（1.2 == 1.2.0）', () => {
		expect(cmpVersion('0.1.0', '0.1.0')).toBe(0)
		expect(cmpVersion('1.2', '1.2.0')).toBe(0)
		expect(cmpVersion('1.2', '1.2.1')).toBeLessThan(0)
	})
})

describe('compat', () => {
	it('apiVersion 大版本不支持 → api 原因', () => {
		expect(compat({ apiVersion: '1.0', minHostVersion: '' }, '0.1.0')).toEqual({ ok: false, reason: 'api' })
	})
	it('minHostVersion 高于宿主 → host 原因；空 = 任意', () => {
		expect(compat({ apiVersion: '0.2', minHostVersion: '9.9.9' }, '0.1.0')).toEqual({ ok: false, reason: 'host' })
		expect(compat({ apiVersion: '0.2', minHostVersion: '' }, '0.1.0')).toEqual({ ok: true })
		expect(compat({ apiVersion: '0.2', minHostVersion: '0.1.0' }, '0.1.0')).toEqual({ ok: true })
	})
})

describe('pickText', () => {
	it('按语言取词，缺失回落另一语言', () => {
		const text = { zh: '中文', en: 'English' }
		expect(pickText(text, 'zh')).toBe('中文')
		expect(pickText(text, 'en')).toBe('English')
		expect(pickText({ zh: '', en: 'only' }, 'zh')).toBe('only')
		expect(pickText({ zh: '只有', en: '' }, 'en')).toBe('只有')
	})
})

describe('sha256Hex', () => {
	it('已知向量（"abc"）', async () => {
		const bytes = new TextEncoder().encode('abc')
		expect(await sha256Hex(bytes.buffer as ArrayBuffer)).toBe(
			'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
		)
	})
})
