import { describe, it, expect, vi } from 'vitest'
import { Schema } from '@milkdown/kit/prose/model'
import {
	ParserState,
	SerializerState,
	type NodeSchema,
	type RemarkParser,
} from '@milkdown/kit/transformer'
import { parseImageBlockAlt, serializeImageBlockAlt } from './imageBlockAlt'

// ---------- 纯 runner 单测（不碰 milkdown 管线，直接断言调用） ----------
describe('parseImageBlockAlt (alt -> caption)', () => {
	it('maps alt into caption and pins ratio to 1', () => {
		const addNode = vi.fn()
		const TYPE = { name: 'image-block' }
		parseImageBlockAlt({ addNode } as never, { url: ':/x', alt: '说明' } as never, TYPE as never)
		expect(addNode).toHaveBeenCalledWith(TYPE, { src: ':/x', caption: '说明', ratio: 1 })
	})

	it('falls back to title, then empty string', () => {
		const addNode = vi.fn()
		parseImageBlockAlt({ addNode } as never, { url: ':/x', title: 'cap' } as never, {} as never)
		expect(addNode).toHaveBeenCalledWith({}, { src: ':/x', caption: 'cap', ratio: 1 })

		addNode.mockClear()
		parseImageBlockAlt({ addNode } as never, { url: ':/x' } as never, {} as never)
		expect(addNode).toHaveBeenCalledWith({}, { src: ':/x', caption: '', ratio: 1 })
	})
})

describe('serializeImageBlockAlt (caption -> alt)', () => {
	it('writes caption into alt and emits no title/ratio', () => {
		const openNode = vi.fn()
		const addNode = vi.fn()
		const closeNode = vi.fn()
		serializeImageBlockAlt(
			{ openNode, addNode, closeNode } as never,
			{ attrs: { src: ':/x', caption: '说明', ratio: 2 } } as never,
		)
		expect(openNode).toHaveBeenCalledWith('paragraph')
		expect(addNode).toHaveBeenCalledWith('image', undefined, undefined, {
			url: ':/x',
			alt: '说明',
		})
		// 关键：不把比例塞进 alt，也不写 title
		const emitted = addNode.mock.calls[0][3] as Record<string, unknown>
		expect(emitted.alt).not.toMatch(/^\d+\.\d+$/)
		expect(emitted).not.toHaveProperty('title')
		expect(closeNode).toHaveBeenCalledOnce()
	})
})

// ---------- 通过真实 @milkdown/kit transformer 的 mdast 往返 ----------
// 用最小 schema（doc/paragraph/text/image-block），image-block 装我们导出的 runner。
// 桩掉 remark 的 parse/runSync/stringify，从而在 mdast 边界注入/取出，无需额外依赖。
function buildSchema() {
	// 显式标注为 Milkdown 的 NodeSchema，让内联 runner 拿到上下文类型（否则 noImplicitAny 报错）。
	const nodes: Record<string, NodeSchema> = {
		doc: {
			content: 'block+',
			parseMarkdown: {
				match: ({ type }) => type === 'root',
				runner: (state, node, type) => state.injectRoot(node, type),
			},
			toMarkdown: {
				match: (node) => node.type.name === 'doc',
				runner: (state, node) => {
					state.openNode('root')
					state.next(node.content)
				},
			},
		},
		paragraph: {
			content: 'inline*',
			group: 'block',
			parseDOM: [{ tag: 'p' }],
			toDOM: () => ['p', 0],
			parseMarkdown: {
				match: (node) => node.type === 'paragraph',
				runner: (state, node, type) => {
					state.openNode(type)
					if (node.children) state.next(node.children)
					state.closeNode()
				},
			},
			toMarkdown: {
				match: (node) => node.type.name === 'paragraph',
				runner: (state, node) => {
					state.openNode('paragraph')
					state.next(node.content)
					state.closeNode()
				},
			},
		},
		text: {
			group: 'inline',
			parseMarkdown: {
				match: ({ type }) => type === 'text',
				runner: (state, node) => state.addText(node.value as string),
			},
			toMarkdown: {
				match: (node) => node.type.name === 'text',
				runner: (state, node) => state.addNode('text', undefined, node.text as string),
			},
		},
		'image-block': {
			inline: false,
			group: 'block',
			atom: true,
			isolating: true,
			marks: '',
			attrs: { src: { default: '' }, caption: { default: '' }, ratio: { default: 1 } },
			parseDOM: [{ tag: 'img[data-type="image-block"]' }],
			toDOM: (node) => ['img', { 'data-type': 'image-block', ...node.attrs }],
			parseMarkdown: { match: ({ type }) => type === 'image-block', runner: parseImageBlockAlt },
			toMarkdown: { match: (node) => node.type.name === 'image-block', runner: serializeImageBlockAlt },
		},
	}
	return new Schema({ nodes, marks: {} })
}

type Mdast = { type: string; children?: Mdast[]; url?: string; alt?: string; title?: string }

function findImage(root: Mdast): Mdast | undefined {
	if (root.type === 'image') return root
	for (const c of root.children ?? []) {
		const hit = findImage(c)
		if (hit) return hit
	}
	return undefined
}

describe('image-block alt round-trip through the real transformer', () => {
	const schema = buildSchema()

	function parse(root: Mdast) {
		const remark = { parse: () => root, runSync: (t: Mdast) => t } as unknown as RemarkParser
		return ParserState.create(schema, remark)('')
	}
	function serialize(doc: ReturnType<typeof parse>): Mdast {
		const remark = { stringify: (m: Mdast) => m } as unknown as RemarkParser
		return SerializerState.create(schema, remark)(doc) as unknown as Mdast
	}

	it('preserves the description (alt) across parse+serialize', () => {
		const root: Mdast = {
			type: 'root',
			children: [{ type: 'image-block', url: ':/abc123', alt: '说明', title: undefined }],
		}
		const doc = parse(root)

		// 解析后 image-block 节点的 caption 应为图片说明
		let caption: unknown
		doc.descendants((n) => {
			if (n.type.name === 'image-block') caption = n.attrs.caption
		})
		expect(caption).toBe('说明')

		// 写回的 image 节点 alt 应为原说明，url 不变，且不出现比例数字
		const img = findImage(serialize(doc))
		expect(img?.url).toBe(':/abc123')
		expect(img?.alt).toBe('说明')
		expect(img?.alt).not.toMatch(/^\d+\.\d+$/)
	})

	it('keeps an empty alt empty (never becomes 1.00)', () => {
		const root: Mdast = {
			type: 'root',
			children: [{ type: 'image-block', url: ':/abc123', alt: '', title: undefined }],
		}
		const img = findImage(serialize(parse(root)))
		expect(img?.url).toBe(':/abc123')
		expect(img?.alt).toBe('')
	})
})
