// UiWidget：server-driven UI 渲染器（spec §9.2/§9.3）。
import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/svelte'
import UiWidget from './UiWidget.svelte'
import type { UiNode } from './api'
import { setLocale } from './i18n.svelte'

const noop = vi.fn(async () => null)

describe('UiWidget', () => {
	it('未知 type 安全忽略（连 children 一起）', () => {
		const node: UiNode = {
			type: 'iframe',
			props: { src: 'https://evil' },
			children: [{ type: 'markdown', props: { source: '**inner**' } }],
		}
		const { container } = render(UiWidget, { props: { node, onCommand: noop } })
		expect(container.textContent?.trim()).toBe('')
		expect(container.querySelector('iframe')).toBeNull()
	})

	it('markdown 节点净化渲染 + children 纵向堆叠', () => {
		const node: UiNode = {
			type: 'markdown',
			props: { source: '**bold** <img src=x onerror=alert(1)>' },
			children: [{ type: 'button', props: { label: 'Go', command: 'c' } }],
		}
		const { container } = render(UiWidget, { props: { node, onCommand: noop } })
		expect(container.querySelector('strong')?.textContent).toBe('bold')
		expect(container.innerHTML).not.toContain('onerror')
		expect(container.querySelector('button')).toBeTruthy()
	})

	it('button 点击 → onCommand(command, props.args)', async () => {
		const onCommand = vi.fn(async () => null)
		const node: UiNode = { type: 'button', props: { label: 'Echo', command: 'echo-args', args: { from: 'ui' } } }
		const { getByRole } = render(UiWidget, { props: { node, onCommand } })
		await fireEvent.click(getByRole('button'))
		expect(onCommand).toHaveBeenCalledWith('echo-args', { from: 'ui' })
	})

	it('list 条目点击 → onCommand(command, {id})；无 command 则纯展示', async () => {
		const onCommand = vi.fn(async () => null)
		const items = [{ id: '1', title: 'one', subtitle: 's' }]
		const withCmd: UiNode = { type: 'list', props: { items, command: 'pick' } }
		const { getByText } = render(UiWidget, { props: { node: withCmd, onCommand } })
		await fireEvent.click(getByText('one'))
		expect(onCommand).toHaveBeenCalledWith('pick', { id: '1' })

		const plain: UiNode = { type: 'list', props: { items } }
		const { container } = render(UiWidget, { props: { node: plain, onCommand: noop } })
		expect(container.querySelector('button')).toBeNull()
	})

	it('form：校验拦截空必填，合法后 onCommand(command, {values})', async () => {
		const onCommand = vi.fn(async () => null)
		const node: UiNode = {
			type: 'form',
			props: {
				fields: { name: { type: 'string', required: true } },
				command: 'save',
				submit_label: '保存',
			},
		}
		const { getByText, container } = render(UiWidget, { props: { node, onCommand } })
		await fireEvent.click(getByText('保存'))
		expect(onCommand).not.toHaveBeenCalled()

		const input = container.querySelector('input[type="text"]') as HTMLInputElement
		await fireEvent.input(input, { target: { value: 'jasper' } })
		await fireEvent.click(getByText('保存'))
		expect(onCommand).toHaveBeenCalledWith('save', { values: { name: 'jasper' } })
	})

	it('文本字段本地化：locale map 按当前语言挑（markdown/button/list/tree）', () => {
		setLocale('zh')
		const node: UiNode = {
			type: 'markdown',
			props: { source: { en: 'English', zh: '中文正文' } },
			children: [
				{ type: 'button', props: { label: { en: 'Save', zh: '保存' }, command: 'c' } },
				{ type: 'list', props: { items: [{ id: '1', title: { en: 'one', zh: '一' } }] } },
				{ type: 'tree', props: { nodes: [{ id: 'a', title: { en: 'root', zh: '根' } }] } },
			],
		}
		const { container } = render(UiWidget, { props: { node, onCommand: noop } })
		const txt = container.textContent ?? ''
		expect(txt).toContain('中文正文')
		expect(txt).toContain('保存')
		expect(txt).toContain('一')
		expect(txt).toContain('根')
		expect(txt).not.toContain('English') // 未选中的语言不出现
		setLocale('en')
	})

	it('缺当前语言的 map 回落 en；纯字符串照旧', () => {
		setLocale('zh') // map 只有 en → 回落 en
		const node: UiNode = {
			type: 'button',
			props: { label: { en: 'OnlyEnglish' }, command: 'c' },
			children: [{ type: 'button', props: { label: 'PlainString', command: 'c2' } }],
		}
		const { container } = render(UiWidget, { props: { node, onCommand: noop } })
		expect(container.textContent).toContain('OnlyEnglish')
		expect(container.textContent).toContain('PlainString')
		setLocale('en')
	})

	it('tree 节点递归渲染并可点击', async () => {
		const onCommand = vi.fn(async () => null)
		const node: UiNode = {
			type: 'tree',
			props: {
				nodes: [{ id: 'a', title: 'root', children: [{ id: 'b', title: 'leaf' }] }],
				command: 'open',
			},
		}
		const { getByText } = render(UiWidget, { props: { node, onCommand } })
		await fireEvent.click(getByText('leaf'))
		expect(onCommand).toHaveBeenCalledWith('open', { id: 'b' })
	})
})
