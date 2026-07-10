// pdf.js 渲染核心（命令式，无框架）：把一个 PDF 渲染进给定容器。
// 内联嵌入（阅读视图占位 / 编辑器 widget）与全屏模态共用同一实现，避免两套 pdf.js 代码。
// pdf.js 核心库 + worker 懒加载（不进首屏）；Canvas 高清（devicePixelRatio）；
// 连续滚动 + IntersectionObserver 懒渲染；工具栏走 app 主题变量。

import { t } from './i18n.svelte'
import { parseResourceId, api } from './api'

export interface PdfHandle {
  destroy(): void
}

// 折叠状态按资源 id 记忆（localStorage）：折叠 = 只留工具栏、收起页面区。
const COLLAPSE_KEY = 'jasper.pdfCollapsed'
function loadCollapsed(): Set<string> {
  try {
    const a = JSON.parse(localStorage.getItem(COLLAPSE_KEY) || '[]')
    return new Set(Array.isArray(a) ? (a as string[]) : [])
  } catch {
    return new Set()
  }
}
function isCollapsed(id: string): boolean {
  return !!id && loadCollapsed().has(id)
}
function setCollapsedPersist(id: string, v: boolean): void {
  if (!id) return
  const s = loadCollapsed()
  if (v) s.add(id)
  else s.delete(id)
  try {
    localStorage.setItem(COLLAPSE_KEY, JSON.stringify([...s]))
  } catch {
    /* 忽略 */
  }
}

export interface PdfRenderOptions {
  url: string // 资源 URL 或 :/id
  name: string
  id?: string // 资源 id（折叠状态持久化的键；url 可能已解析成 /api/... 拿不到 id 时用它）
  fullscreen?: boolean // true=全屏模态（关闭按钮）；false=内联（展开按钮）
  onExpand?: () => void // 内联：点「展开」→ 打开全屏
  onClose?: () => void // 全屏：点「关闭」
}

// 内联遮罩样式：不依赖 Icon.svelte 的作用域 .icon 类（本渲染器产出的 DOM 在 {@html}/CM widget
// 里，拿不到组件作用域样式），故直接把 mask 样式写死在元素上。
function iconEl(name: string, size = 16): HTMLElement {
  const i = document.createElement('i')
  const mask = `var(--icon-${name}) center/contain no-repeat`
  i.style.cssText =
    `display:inline-block;flex:0 0 auto;width:${size}px;height:${size}px;` +
    `background-color:currentColor;-webkit-mask:${mask};mask:${mask};`
  return i
}

function toolButton(icon: string, label: string, onClick: () => void, size = 16): HTMLButtonElement {
  const b = document.createElement('button')
  b.type = 'button'
  b.className = 'pt'
  b.setAttribute('aria-label', label)
  b.title = label
  b.appendChild(iconEl(icon, size))
  b.addEventListener('click', onClick)
  return b
}

export function renderPdfInto(container: HTMLElement, opts: PdfRenderOptions): PdfHandle {
  const rid = parseResourceId(opts.url)
  const url = rid ? api.resourceUrl(rid) : opts.url
  const fullscreen = !!opts.fullscreen
  const dpr = Math.min(window.devicePixelRatio || 1, 2)

  const collapseId = opts.id || rid || ''
  let collapsed = !fullscreen && !!collapseId && isCollapsed(collapseId)
  const root = document.createElement('div')
  root.className = 'pdf-viewer' + (fullscreen ? ' fullscreen' : ' inline') + (collapsed ? ' collapsed' : '')

  // ---- 工具栏 ----
  const toolbar = document.createElement('div')
  toolbar.className = 'pdf-toolbar'
  const nameEl = document.createElement('span')
  nameEl.className = 'pdf-name'
  nameEl.textContent = opts.name
  nameEl.title = opts.name
  const tools = document.createElement('div')
  tools.className = 'pdf-tools'

  const scaleLabel = document.createElement('span')
  scaleLabel.className = 'pdf-scale'
  const pageLabel = document.createElement('span')
  pageLabel.className = 'pdf-page'

  const sep = () => {
    const s = document.createElement('span')
    s.className = 'pdf-sep'
    return s
  }
  const zoomOut = toolButton('minus', t('pdf.zoomOut'), () => zoom(-0.15))
  const zoomIn = toolButton('plus', t('pdf.zoomIn'), () => zoom(0.15))
  const prev = toolButton('chevron-up', t('pdf.prev'), () => goto(curPage - 1))
  const next = toolButton('chevron-down', t('pdf.next'), () => goto(curPage + 1))
  const download = toolButton('download', t('pdf.download'), () => {
    const a = document.createElement('a')
    a.href = url
    a.download = opts.name
    a.click()
  })
  const lastBtn = fullscreen
    ? toolButton('close', t('common.close'), () => opts.onClose?.(), 17)
    : toolButton('maximize', t('pdf.expand'), () => opts.onExpand?.())

  tools.append(zoomOut, scaleLabel, zoomIn, sep(), prev, pageLabel, next, sep(), download, lastBtn)

  // 折叠按钮（仅内联）：收起页面区、只留工具栏；状态按资源 id 记忆
  const left = document.createElement('div')
  left.className = 'pdf-left'
  if (!fullscreen) {
    const collapseBtn = toolButton(collapsed ? 'chevrons-up-down' : 'chevrons-down-up', t('pdf.collapse'), () => {
      collapsed = !collapsed
      root.classList.toggle('collapsed', collapsed)
      collapseBtn.replaceChildren(iconEl(collapsed ? 'chevrons-up-down' : 'chevrons-down-up'))
      if (collapseId) setCollapsedPersist(collapseId, collapsed)
    })
    left.append(collapseBtn)
  }
  left.append(nameEl)
  toolbar.append(left, tools)

  // ---- 页面区 ----
  const body = document.createElement('div')
  body.className = 'pdf-body'
  root.append(toolbar, body)
  container.appendChild(root)

  // ---- 状态 ----
  let scale = 1
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let pdf: any = null
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let loadingTask: any = null
  let observer: IntersectionObserver | undefined
  let numPages = 0
  let curPage = 1
  let destroyed = false
  const pageEls: HTMLDivElement[] = []
  const rendered = new Set<number>()
  let base = { w: 800, h: 1035 }

  function updateLabels() {
    scaleLabel.textContent = `${Math.round(scale * 100)}%`
    pageLabel.textContent = `${curPage} / ${numPages || '—'}`
    prev.disabled = curPage <= 1
    next.disabled = curPage >= numPages
  }

  async function renderPage(i: number) {
    if (!pdf || rendered.has(i) || destroyed) return
    rendered.add(i)
    try {
      const page = await pdf.getPage(i)
      if (destroyed) return
      const vp = page.getViewport({ scale: scale * dpr })
      const canvas = pageEls[i - 1]?.querySelector('canvas') as HTMLCanvasElement | null
      if (!canvas) return
      canvas.width = vp.width
      canvas.height = vp.height
      canvas.style.width = `${vp.width / dpr}px`
      canvas.style.height = `${vp.height / dpr}px`
      const ctx = canvas.getContext('2d')
      if (ctx) await page.render({ canvas, canvasContext: ctx, viewport: vp }).promise
    } catch {
      rendered.delete(i)
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
            curPage = i
            updateLabels()
          }
        }
      },
      { root: body, rootMargin: '500px 0px' },
    )
    for (const el of pageEls) if (el) observer.observe(el)
  }

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
    updateLabels()
    observePages()
  }

  function zoom(delta: number) {
    scale = Math.min(3, Math.max(0.3, +(scale + delta).toFixed(2)))
    applyScale()
  }

  function goto(i: number) {
    const n = Math.min(numPages, Math.max(1, i))
    pageEls[n - 1]?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  function showMessage(text: string, isErr = false) {
    body.innerHTML = ''
    const m = document.createElement('div')
    m.className = 'pdf-msg' + (isErr ? ' err' : '')
    m.textContent = text
    body.appendChild(m)
  }

  showMessage(t('pdf.loading'))

  ;(async () => {
    try {
      const pdfjs = await import('pdfjs-dist')
      const workerUrl = (await import('pdfjs-dist/build/pdf.worker.min.mjs?url')).default
      pdfjs.GlobalWorkerOptions.workerSrc = workerUrl
      loadingTask = pdfjs.getDocument({ url })
      pdf = await loadingTask.promise
      if (destroyed) {
        void loadingTask.destroy()
        return
      }
      numPages = pdf.numPages
      const p1 = await pdf.getPage(1)
      const vp = p1.getViewport({ scale: 1 })
      base = { w: vp.width, h: vp.height }
      // 初始按容器宽度自适应（内联更贴合正文；全屏给一点边距）
      const avail = Math.max(320, body.clientWidth - (fullscreen ? 48 : 24))
      scale = Math.min(fullscreen ? 1.6 : 1.4, +(avail / base.w).toFixed(3))

      body.innerHTML = ''
      for (let i = 1; i <= numPages; i++) {
        const box = document.createElement('div')
        box.className = 'pdf-page-box'
        box.dataset.page = String(i)
        box.appendChild(document.createElement('canvas'))
        body.appendChild(box)
        pageEls[i - 1] = box
      }
      applyScale()
    } catch (e) {
      if (!destroyed) showMessage(`${t('pdf.error')}: ${(e as Error)?.message ?? e}`, true)
    }
  })()

  return {
    destroy() {
      destroyed = true
      observer?.disconnect()
      try {
        void loadingTask?.destroy?.()
      } catch {
        /* ignore */
      }
      root.remove()
    },
  }
}
