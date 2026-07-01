<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { api, parseResourceId } from './api'
  import { withImageAltCaption } from './milkdown/imageBlockAlt'
  import { t } from './i18n.svelte'

  let {
    value,
    onChange,
  }: {
    value: string
    onChange: (v: string) => void
  } = $props()

  let host: HTMLDivElement
  let crepe: import('@milkdown/crepe').Crepe | undefined
  // 闸门：create() 完成前（含初次解析/规范化）不回调 onChange，
  // 确保「仅打开/切换到富文本」不会触发自动保存把笔记重排写回。
  let ready = false
  let loadError = $state('')

  // 给无名 Blob（截图粘贴）造个带扩展名的文件名
  function nameOf(file: File): string {
    if (file.name) return file.name
    const ext = (file.type.split('/')[1] || 'bin').replace('+xml', '')
    return `pasted.${ext}`
  }

  onMount(async () => {
    try {
      // 懒加载 Crepe（含 ProseMirror/remark/组件层）+ 主题，单独成 chunk，不进首屏。
      // imageBlockSchema 来自 Crepe 内部同一份 @milkdown/kit，随 Crepe chunk 走，不额外加重首屏。
      const [{ Crepe }, { imageBlockSchema }] = await Promise.all([
        import('@milkdown/crepe'),
        import('@milkdown/kit/component/image-block'),
        import('@milkdown/crepe/theme/common/style.css'),
        import('@milkdown/crepe/theme/classic.css'),
      ])
      crepe = new Crepe({
        root: host,
        defaultValue: value, // 存储态 markdown（:/id 原样喂入，remark 当普通 URL 保留）
        features: {
          [Crepe.Feature.Latex]: true, // 数学 $…$ / $$…$$
        },
        featureConfigs: {
          [Crepe.Feature.Placeholder]: { text: t('editor.hint') },
          [Crepe.Feature.ImageBlock]: {
            // 粘贴/选择图片 → 上传为资源 → 文档里存 Joplin 规范的 :/id（非绝对 URL）
            onUpload: async (file: File) => {
              const r = await api.uploadResource(file, nameOf(file))
              return `:/${r.id}`
            },
            // 仅渲染 <img> 时把 :/id 解析成真实地址；底层 markdown 模型不变 → getMarkdown 仍输出 :/id
            proxyDomURL: (url: string) => {
              const id = parseResourceId(url)
              return id ? api.resourceUrl(id) : url
            },
          },
        },
      })
      crepe.on((listener) => {
        listener.markdownUpdated((_ctx, markdown) => {
          if (ready) onChange(markdown)
        })
      })
      // 覆盖 Crepe 图片块(image-block)的 markdown 解析/写回，恢复 alt(图片说明)语义
      // （默认实现把 alt 当缩放比例，写回 ![1.00](:/id) 毁掉说明）。见 milkdown/imageBlockAlt.ts。
      // 同名 image-block 后注册者胜出（@milkdown/utils $node 按 id 覆盖），故必须在 create() 前 use。
      crepe.editor.use(withImageAltCaption(imageBlockSchema))
      await crepe.create()
      ready = true
    } catch (e) {
      loadError = `${t('editor.wysiwygFailed')}: ${(e as Error)?.message ?? e}`
    }
  })

  onDestroy(() => {
    ready = false
    crepe?.destroy()
  })
</script>

<div class="wys" bind:this={host}></div>
{#if loadError}<div class="wys-err">{loadError}</div>{/if}

<style>
  .wys {
    height: 100%;
    min-height: 0;
    overflow-y: auto;
  }
  .wys-err {
    padding: 10px 16px;
    color: var(--danger);
    font-size: 13px;
  }
  /* 把 Crepe 主题色映射到 app 调色板（随明暗自动切换）。
     用 .wys 后代选择器抬高优先级，确保覆盖主题默认值、且不依赖样式导入顺序。 */
  :global(.wys .milkdown) {
    --crepe-color-background: var(--bg);
    --crepe-color-on-background: var(--text);
    --crepe-color-surface: var(--bg);
    --crepe-color-surface-low: var(--bg-side);
    --crepe-color-on-surface: var(--text);
    --crepe-color-on-surface-variant: var(--text-dim);
    --crepe-color-primary: var(--accent);
    --crepe-color-secondary: var(--accent-soft);
    --crepe-color-outline: var(--border);
    --crepe-color-hover: var(--hover);
    --crepe-color-selected: var(--accent-soft);
    --crepe-color-inline-area: var(--code-bg);
    --crepe-font-default: inherit;
    --crepe-font-title: inherit;
    --crepe-font-code: 'SF Mono', Menlo, Consolas, monospace;
    background: var(--bg);
    height: 100%;
  }
  :global(.wys .milkdown .ProseMirror) {
    min-height: 100%;
    outline: none;
    padding: 4px 0 48px;
  }

  /* ---------- 深色/低对比主题下的可读性加强 ----------
     Crepe 默认把表格边框、分隔线做成 outline 色的 20% 透明（transparent 80%），
     复选框/列表序号又直接用较暗的 outline 色作字形——暗背景下几乎不可见。
     这里把这些对比关键的元素拉回与只读渲染视图(.content)一致的实心 --border/
     --text-dim/--accent，改善所有主题（尤其深色），并保持编辑态与阅读态观感统一。
     选择器多带一个 .wys → 特异性高于 Crepe 主题默认，稳定覆盖。 */

  /* 表格：实心边框 + 表头底色（原为 20% 透明，几乎看不见） */
  :global(.wys .milkdown .milkdown-table-block th),
  :global(.wys .milkdown .milkdown-table-block td) {
    border-color: var(--border);
  }
  :global(.wys .milkdown .milkdown-table-block th) {
    background: var(--bg-side);
  }

  /* 分隔线 hr：实心（原为 20% 透明） */
  :global(.wys .milkdown .ProseMirror hr) {
    background-color: var(--border);
  }

  /* 复选框/列表序号字形：outline 太暗，改用 text-dim 提升对比 */
  :global(.wys .milkdown .milkdown-list-item-block li .label-wrapper) {
    color: var(--text-dim);
  }
  :global(.wys .milkdown .milkdown-list-item-block li .label-wrapper svg) {
    fill: var(--text-dim);
  }
  /* 已勾选的复选框用强调色，一眼可辨已完成 */
  :global(.wys .milkdown .milkdown-list-item-block li .label-wrapper .checked),
  :global(
      .wys .milkdown .milkdown-list-item-block li .label-wrapper .checked svg
    ) {
    color: var(--accent);
    fill: var(--accent);
  }
</style>
