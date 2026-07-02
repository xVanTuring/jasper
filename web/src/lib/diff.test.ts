import { describe, it, expect } from 'vitest'
import { diffLines } from './diff'

describe('diffLines', () => {
	it('相同文本 → 全 same', () => {
		const d = diffLines('a\nb', 'a\nb')
		expect(d).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'same', text: 'b' },
		])
	})

	it('中段替换：保留公共前后缀，改动行成对 del/add', () => {
		const d = diffLines('头\n旧行\n尾', '头\n新行\n尾')
		expect(d).toEqual([
			{ type: 'same', text: '头' },
			{ type: 'del', text: '旧行' },
			{ type: 'add', text: '新行' },
			{ type: 'same', text: '尾' },
		])
	})

	it('插入与删除', () => {
		expect(diffLines('a\nc', 'a\nb\nc')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'add', text: 'b' },
			{ type: 'same', text: 'c' },
		])
		expect(diffLines('a\nb\nc', 'a\nc')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'del', text: 'b' },
			{ type: 'same', text: 'c' },
		])
	})

	it('空 → 全部新增（create 提案）', () => {
		expect(diffLines('', 'x\ny')).toEqual([
			{ type: 'del', text: '' },
			{ type: 'add', text: 'x' },
			{ type: 'add', text: 'y' },
		])
	})

	it('超预算大文本退化为整段替换但不丢行', () => {
		const a = Array.from({ length: 600 }, (_, i) => `a${i}`).join('\n')
		const b = Array.from({ length: 600 }, (_, i) => `b${i}`).join('\n')
		const d = diffLines(a, b)
		expect(d.filter((l) => l.type === 'del').length).toBe(600)
		expect(d.filter((l) => l.type === 'add').length).toBe(600)
	})
})
