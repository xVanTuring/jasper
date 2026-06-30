<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { api, parseResourceId } from './api'
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
      const [{ Crepe }] = await Promise.all([
        import('@milkdown/crepe'),
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
</style>
