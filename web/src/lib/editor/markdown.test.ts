import { describe, it, expect } from 'vitest'
import { formatMarkdown, toggleBlockLines } from './markdown'

describe('toggleBlockLines', () => {
	it('adds a heading prefix, and removes it when re-applied', () => {
		expect(toggleBlockLines(['hello'], 'h2')).toEqual(['## hello'])
		expect(toggleBlockLines(['## hello'], 'h2')).toEqual(['hello'])
	})

	it('switches heading level instead of stacking', () => {
		expect(toggleBlockLines(['# hi'], 'h2')).toEqual(['## hi'])
	})

	it('numbers ordered lists sequentially', () => {
		expect(toggleBlockLines(['a', 'b', 'c'], 'ordered')).toEqual(['1. a', '2. b', '3. c'])
	})

	it('converts between list types without doubling markers', () => {
		expect(toggleBlockLines(['- a', '- b'], 'ordered')).toEqual(['1. a', '2. b'])
		expect(toggleBlockLines(['1. a'], 'task')).toEqual(['- [ ] a'])
		expect(toggleBlockLines(['- [ ] a'], 'bullet')).toEqual(['- a'])
	})

	it('toggles a bullet list off', () => {
		expect(toggleBlockLines(['- a', '- b'], 'bullet')).toEqual(['a', 'b'])
	})

	it('only toggles off when every line already matches', () => {
		// mixed → normalize all to bullets (not toggle off)
		expect(toggleBlockLines(['- a', 'b'], 'bullet')).toEqual(['- a', '- b'])
	})
})

describe('formatMarkdown', () => {
	it('aligns a simple GFM table', () => {
		const src = '| a | bb |\n|-|-|\n| 1 | 2 |'
		expect(formatMarkdown(src)).toBe('| a | bb |\n| - | -- |\n| 1 | 2  |\n')
	})

	it('respects column alignment markers', () => {
		const src = '| x | y |\n|:-:|--:|\n| a | bb |'
		expect(formatMarkdown(src)).toBe('|  x  |  y |\n| :-: | -: |\n|  a  | bb |\n')
	})

	it('accounts for CJK double width when aligning', () => {
		const src = '| 名称 | v |\n|-|-|\n| a | 值 |'
		expect(formatMarkdown(src)).toBe('| 名称 | v  |\n| ---- | -- |\n| a    | 值 |\n')
	})

	it('normalizes heading spacing and bullet markers', () => {
		expect(formatMarkdown('##  Title')).toBe('## Title\n')
		expect(formatMarkdown('* item\n+ item2')).toBe('- item\n- item2\n')
	})

	it('collapses blank runs and trims edges', () => {
		expect(formatMarkdown('\n\na\n\n\n\nb\n\n')).toBe('a\n\nb\n')
	})

	it('leaves fenced code blocks untouched', () => {
		const src = '```\n*  keep  this *\n\n\nverbatim\n```\n'
		expect(formatMarkdown(src)).toBe('```\n*  keep  this *\n\n\nverbatim\n```\n')
	})
})
