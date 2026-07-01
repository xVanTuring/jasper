// Crepe(Milkdown) 图片块(image-block)的 alt 语义修复。
//
// Crepe 默认把 markdown 的 alt 槽当作图片「缩放比例」、把 title 槽当作 caption：
// 于是 Joplin/CommonMark 的 `![说明](:/id)` 进编辑器时 `说明` 被当成比例（NaN→1），
// 保存又写回 `![1.00](:/id)`，图片说明文字被毁。
//
// 这里覆盖 image-block 的 parseMarkdown/toMarkdown：
//   解析：alt → caption（可见可编辑），比例恒为默认 1（不再从 alt 读）
//   写回：caption → alt，不写 title、不写比例
// 从而恢复 `![说明](:/id)` 的原语义。代价：缩放比例不落盘（Joplin 也无处存放）。
//
// milkdown 只做 **type-only** 引入（编译期擦除），本模块无运行时 milkdown 依赖，
// 故不破坏 WysiwygEditor 对 Crepe 的懒加载（imageBlockSchema 由调用方动态 import 后传入）。
import type { imageBlockSchema as ImageBlockSchemaPlugin } from '@milkdown/kit/component/image-block'
import type { NodeSchema } from '@milkdown/kit/transformer'

type ImageBlockSchema = typeof ImageBlockSchemaPlugin

/** 解析 runner：把 image-block 的 alt（图片说明）落到 caption，比例恒为 1。 */
export const parseImageBlockAlt: NodeSchema['parseMarkdown']['runner'] = (state, node, type) => {
	state.addNode(type, {
		src: (node.url as string) ?? '',
		caption: (node.alt as string) || (node.title as string) || '',
		ratio: 1,
	})
}

/** 写回 runner：把 caption 原样写回 alt，不写 title、不写比例。 */
export const serializeImageBlockAlt: NodeSchema['toMarkdown']['runner'] = (state, node) => {
	state.openNode('paragraph')
	state.addNode('image', undefined, undefined, {
		url: node.attrs.src,
		alt: String(node.attrs.caption ?? ''),
	})
	state.closeNode()
}

/**
 * 扩展 Crepe 的 image-block schema，使 alt 与 caption 互相对应（恢复图片说明语义）。
 * 返回的插件需在 `crepe.create()` 之前 `crepe.editor.use(...)`：同名节点后注册者胜出
 * （见 @milkdown/utils 的 $node：按 id 覆盖 nodesCtx）。
 */
export function withImageAltCaption(imageBlockSchema: ImageBlockSchema): ImageBlockSchema {
	return imageBlockSchema.extendSchema((prev) => (ctx) => {
		const base = prev(ctx)
		return {
			...base,
			parseMarkdown: {
				match: base.parseMarkdown.match,
				runner: parseImageBlockAlt,
			},
			toMarkdown: {
				match: base.toMarkdown.match,
				runner: serializeImageBlockAlt,
			},
		}
	})
}
