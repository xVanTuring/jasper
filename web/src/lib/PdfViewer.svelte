<script lang="ts">
  // 全屏 PDF 模态：薄壳，实际渲染交给 pdfRender.ts 的 renderPdfInto（与内联嵌入共用）。
  import { onMount, onDestroy } from 'svelte'
  import { renderPdfInto, type PdfHandle } from './pdfRender'

  let { url, name = 'PDF', onClose }: { url: string; name?: string; onClose: () => void } = $props()

  let host = $state<HTMLDivElement>()
  let handle: PdfHandle | undefined

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose()
  }

  onMount(() => {
    if (host) handle = renderPdfInto(host, { url, name, fullscreen: true, onClose })
  })
  onDestroy(() => handle?.destroy())
</script>

<svelte:window onkeydown={onKey} />
<div class="pdf-overlay" role="dialog" aria-modal="true" aria-label={name} bind:this={host}></div>

<style>
  .pdf-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: color-mix(in srgb, var(--bg) 92%, black);
    display: flex;
  }
</style>
