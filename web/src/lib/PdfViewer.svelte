<script lang="ts">
  // 自绘 PDF 阅读器：pdf.js v6 核心库（懒加载，不进首屏）渲染到 canvas，
  // 连续滚动 + 懒渲染（IntersectionObserver，翻到才渲染），工具栏走 app 主题变量。
  // 作为全屏模态，App 在需要时挂载。不用 pdf.js 自带的 viewer chrome。
  import { onMount, onDestroy, tick } from 'svelte'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import type { PDFDocumentProxy, PDFDocumentLoadingTask } from 'pdfjs-dist'

  let { url, name = 'PDF', onClose }: { url: string; name?: string; onClose: () => void } = $props()

  let scroller = $state<HTMLDivElement>()
  let pdf: PDFDocumentProxy | null = null
  let loadingTask: PDFDocumentLoadingTask | null = null
  let numPages = $state(0)
  let curPage = $state(1)
  let scale = $state(1.2)
  let loading = $state(true)
  let error = $state('')

  const pageEls: HTMLDivElement[] = []
  const rendered = new Set<number>()
  let base = { w: 800, h: 1035 } // 第 1 页在 scale=1 的尺寸（用于占位盒；多数 PDF 各页等大）
  let observer: IntersectionObserver | undefined
  const dpr = Math.min(window.devicePixelRatio || 1, 2)

  async function renderPage(i: number) {
    if (!pdf || rendered.has(i)) return
    rendered.add(i)
    try {
      const page = await pdf.getPage(i)
      const vp = page.getViewport({ scale: scale * dpr })
      const holder = pageEls[i - 1]
      const canvas = holder?.querySelector('canvas') as HTMLCanvasElement | null
      if (!canvas) return
      canvas.width = vp.width
      canvas.height = vp.height
      canvas.style.width = `${vp.width / dpr}px`
      canvas.style.height = `${vp.height / dpr}px`
      const ctx = canvas.getContext('2d')
      if (ctx) await page.render({ canvas, canvasContext: ctx, viewport: vp }).promise
    } catch {
      rendered.delete(i) // 失败可重试
    }
  }

  function observePages() {
    observer?.disconnect()
    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const i = Number((e.target as HTMLElement).dataset.page)
          if (e.isIntersecting) {
            renderPage(i)
            curPage = i // 进入视口的最后一页作为当前页指示
          }
        }
      },
      { root: scroller, rootMargin: '400px 0px' },
    )
    for (const el of pageEls) if (el) observer.observe(el)
  }

  // 缩放变化：清空已渲染 + 重设占位尺寸 + 重新观察（可见页按新比例重绘）
  function applyScale() {
    rendered.clear()
    for (const el of pageEls) {
      if (!el) continue
      el.style.width = `${base.w * scale}px`
      el.style.height = `${base.h * scale}px`
      const c = el.querySelector('canvas') as HTMLCanvasElement | null
      if (c) {
        c.width = 0
        c.style.width = `${base.w * scale}px`
        c.style.height = `${base.h * scale}px`
      }
    }
    observePages()
  }

  function zoom(delta: number) {
    scale = Math.min(3, Math.max(0.4, +(scale + delta).toFixed(2)))
  }
  let scaleInited = false
  $effect(() => {
    void scale
    if (scaleInited) applyScale()
  })

  function goto(i: number) {
    const n = Math.min(numPages, Math.max(1, i))
    pageEls[n - 1]?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose()
  }

  onMount(async () => {
    try {
      const pdfjs = await import('pdfjs-dist')
      const workerUrl = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default
      pdfjs.GlobalWorkerOptions.workerSrc = workerUrl
      loadingTask = pdfjs.getDocument({ url })
      pdf = await loadingTask.promise
      numPages = pdf.numPages
      const p1 = await pdf.getPage(1)
      const vp = p1.getViewport({ scale: 1 })
      base = { w: vp.width, h: vp.height }
      loading = false
      await tick() // 等页占位盒渲染出来再观察
      applyScale()
      scaleInited = true
    } catch (e) {
      error = (e as Error)?.message || `${e}`
      loading = false
    }
  })

  onDestroy(() => {
    observer?.disconnect()
    void loadingTask?.destroy()
  })
</script>

<svelte:window onkeydown={onKey} />

<div class="pdf-overlay" role="dialog" aria-modal="true" aria-label={name}>
  <div class="pdf-toolbar">
    <span class="pdf-name" title={name}>{name}</span>
    <div class="pdf-tools">
      <button type="button" class="pt" aria-label={t('pdf.zoomOut')} onclick={() => zoom(-0.2)}>
        <Icon name="minus" size={16} />
      </button>
      <span class="pdf-scale">{Math.round(scale * 100)}%</span>
      <button type="button" class="pt" aria-label={t('pdf.zoomIn')} onclick={() => zoom(0.2)}>
        <Icon name="plus" size={16} />
      </button>
      <span class="pdf-sep"></span>
      <button type="button" class="pt" aria-label={t('pdf.prev')} disabled={curPage <= 1} onclick={() => goto(curPage - 1)}>
        <Icon name="chevron-up" size={16} />
      </button>
      <span class="pdf-page">{curPage} / {numPages || '—'}</span>
      <button type="button" class="pt" aria-label={t('pdf.next')} disabled={curPage >= numPages} onclick={() => goto(curPage + 1)}>
        <Icon name="chevron-down" size={16} />
      </button>
      <span class="pdf-sep"></span>
      <a class="pt" href={url} download={name} aria-label={t('pdf.download')}><Icon name="download" size={16} /></a>
      <button type="button" class="pt" aria-label={t('common.close')} onclick={onClose}><Icon name="close" size={17} /></button>
    </div>
  </div>

  <div class="pdf-body" bind:this={scroller}>
    {#if loading}
      <div class="pdf-msg">{t('pdf.loading')}</div>
    {:else if error}
      <div class="pdf-msg err">{t('pdf.error')}: {error}</div>
    {:else}
      {#each Array(numPages) as _, i (i)}
        <div class="pdf-page-box" data-page={i + 1} bind:this={pageEls[i]}>
          <canvas></canvas>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .pdf-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--bg) 92%, black);
  }
  .pdf-toolbar {
    flex: 0 0 auto;
    height: 46px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 12px;
    background: var(--bg-side);
    border-bottom: 1px solid var(--border);
  }
  .pdf-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .pdf-tools {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: 0 0 auto;
  }
  .pt {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: none;
    background: none;
    color: var(--text-dim);
    border-radius: 7px;
    cursor: pointer;
    text-decoration: none;
  }
  .pt:hover {
    background: var(--hover);
    color: var(--text);
  }
  .pt:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .pdf-scale,
  .pdf-page {
    font-size: 12px;
    color: var(--text-dim);
    padding: 0 6px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .pdf-sep {
    width: 1px;
    height: 18px;
    background: var(--border);
    margin: 0 6px;
  }
  .pdf-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 18px 12px 40px;
  }
  .pdf-page-box {
    background: #fff;
    box-shadow: 0 2px 14px rgba(0, 0, 0, 0.35);
    border-radius: 2px;
    flex: 0 0 auto;
  }
  .pdf-page-box canvas {
    display: block;
  }
  .pdf-msg {
    color: var(--text-dim);
    font-size: 13px;
    margin: 40px 0;
  }
  .pdf-msg.err {
    color: var(--danger);
  }
</style>
