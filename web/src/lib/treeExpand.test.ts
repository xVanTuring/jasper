// treeExpand（笔记本树展开/折叠记忆）单测：jsdom 提供 localStorage。
import { beforeEach, describe, expect, it } from 'vitest'
import { isExpanded, setExpanded, toggleExpanded, _resetForTest } from './treeExpand.svelte'

const KEY = 'jasper.expandedFolders'

// 每个用例前清内存态 + 持久值，保证隔离。
beforeEach(() => {
	_resetForTest()
})

describe('treeExpand', () => {
	it('默认全部折叠', () => {
		expect(isExpanded('a')).toBe(false)
	})

	it('toggle 展开/折叠往返', () => {
		toggleExpanded('a')
		expect(isExpanded('a')).toBe(true)
		toggleExpanded('a')
		expect(isExpanded('a')).toBe(false)
	})

	it('setExpanded 幂等', () => {
		setExpanded('a', true)
		setExpanded('a', true)
		expect(isExpanded('a')).toBe(true)
		setExpanded('a', false)
		setExpanded('a', false)
		expect(isExpanded('a')).toBe(false)
	})

	it('展开态持久化到 localStorage（只存展开的 id）', () => {
		setExpanded('a', true)
		setExpanded('b', true)
		setExpanded('a', false)
		expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual(['b'])
	})

	it('从 localStorage 恢复（reset 后不再展开）', () => {
		setExpanded('a', true)
		expect(localStorage.getItem(KEY)).toContain('a')
		_resetForTest()
		expect(isExpanded('a')).toBe(false)
		expect(localStorage.getItem(KEY)).toBeNull()
	})

	it('容忍损坏的持久值（不抛）', () => {
		localStorage.setItem(KEY, '{not json')
		// load() 已在模块初始化时跑过；这里断言写入不受污染值影响
		expect(() => setExpanded('a', true)).not.toThrow()
		expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual(['a'])
	})
})
