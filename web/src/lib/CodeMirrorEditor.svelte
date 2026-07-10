<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { api } from './api'
  import { t } from './i18n.svelte'
  import Button from './Button.svelte'
  import type { EditorHandle, EditorMode } from './editor/types'
  import type { EditorController } from './editor/build'
  import { editorInputPlugins } from './plugins.svelte'

  let {
    value,
    onChange,
    onReady,
    mode = 'live',
    readOnly = false,
  }: {
    value: string
    onChange: (v: string) => void
    // 就绪后回传编辑器句柄，供工具栏/插件命令操作源码（父级持有）
    onReady?: (h: EditorHandle) => void
    mode?: EditorMode
    readOnly?: boolean
  } = $props()

  let host: HTMLDivElement
  let fileInput: HTMLInputElement
  let ctrl: EditorController | undefined

  let uploading = $state(0)
  let uploadErr = $state('')

  // ---------- 编辑期插件钩子（spec §3.7 contributes.editor，phase="input"） ----------
  // 用户输入停顿后把整段源码依次交给声明了 input 相位的插件的 editor.transform 改写。
  // 统一到 CM6 后源码/Live Preview 同一实例，两模式均接入（保守整篇替换 + 陈旧保护）。
  const TRANSFORM_DEBOUNCE_MS = 700
  let transformTimer: ReturnType<typeof setTimeout> | undefined

  function scheduleTransform() {
    if (!ctrl || editorInputPlugins().length === 0) return
    clearTimeout(transformTimer)
    transformTimer = setTimeout(runTransforms, TRANSFORM_DEBOUNCE_MS)
  }

  async function runTransforms() {
    if (!ctrl) return
    const ids = editorInputPlugins()
    if (ids.length === 0) return
    const view = ctrl.view
    const sent = view.state.doc.toString()
    const anchor = view.state.selection.main.anchor
    let out = sent
    for (const id of ids) {
      try {
        out = await api.editorTransform(id, 'input', out)
      } catch {
        /* 插件禁用/网络/只读等 → 跳过该插件 */
      }
    }
    if (!ctrl) return
    // 陈旧保护：等待期间用户又敲了字 → 丢弃，绝不覆盖新输入
    if (view.state.doc.toString() !== sent || out === sent) return
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: out },
      selection: { anchor: Math.min(anchor, out.length) },
    })
  }

  // ---------- 文件上传（附件按钮 / 粘贴 / 拖拽） ----------
  function nameOf(file: File): string {
    if (file.name) return file.name
    const ext = (file.type.split('/')[1] || 'bin').replace('+xml', '')
    return `pasted-${Date.now()}.${ext}`
  }

  async function uploadFiles(files: File[]) {
    if (!files.length || !ctrl) return
    uploadErr = ''
    for (const file of files) {
      uploading++
      try {
        const r = await api.uploadResource(file, nameOf(file))
        ctrl.handle.insert(r.markdown + '\n')
      } catch (e) {
        uploadErr = (e as Error).message || t('editor.uploadFailed')
      } finally {
        uploading--
      }
    }
  }

  function pickFile() {
    fileInput?.click()
  }
  function onPick() {
    if (fileInput.files) uploadFiles(Array.from(fileInput.files))
    fileInput.value = ''
  }

  onMount(async () => {
    // 惰性加载 CodeMirror 全套（含 Live Preview/widgets）——单独成 chunk，不进首屏
    const { createEditor } = await import('./editor/build')
    ctrl = createEditor({
      parent: host,
      doc: value,
      mode,
      readOnly,
      placeholderText: t('editor.placeholder'),
      onChange: (v, userEvent) => {
        onChange(v)
        if (userEvent) scheduleTransform()
      },
      onFiles: uploadFiles,
    })
    ctrl.view.focus()
    onReady?.(ctrl.handle)
  })

  // 模式 / 只读 变化 → 热切换（同一实例，不重建、不丢内容与光标）
  $effect(() => {
    void mode
    ctrl?.setMode(mode)
  })
  $effect(() => {
    void readOnly
    ctrl?.setReadOnly(readOnly)
  })

  onDestroy(() => {
    clearTimeout(transformTimer)
    ctrl?.destroy()
    ctrl = undefined
  })
</script>

<div class="editor-col">
  <div class="ed-toolbar">
    <Button variant="default" icon="attach" label={t('editor.attach')} onclick={pickFile} disabled={uploading > 0 || readOnly} />
    <span class="hint">{t('editor.hint')}</span>
    {#if uploading > 0}<span class="up">{t('editor.uploading', { n: uploading })}</span>{/if}
    {#if uploadErr}<span class="err">{uploadErr}</span>{/if}
    <input type="file" multiple bind:this={fileInput} onchange={onPick} hidden />
  </div>
  <div class="cm-host" bind:this={host}></div>
</div>

<style>
  .editor-col {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .ed-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 2px 8px;
    flex: 0 0 auto;
  }
  .hint,
  .up {
    font-size: 12px;
    color: var(--text-dim);
  }
  .err {
    font-size: 12px;
    color: var(--danger);
  }
  .cm-host {
    flex: 1;
    min-height: 0;
  }
  :global(.cm-host .cm-editor) {
    height: 100%;
  }
  /* 选区色：强制覆盖 CM drawSelection 的 &light/&dark 基础默认（其特异性更高，
     否则暗色下用 CM 自带浅色选区、近白正文压上去看不见）。半透明强调色明暗都够对比。 */
  :global(.cm-host .cm-selectionBackground) {
    background: color-mix(in srgb, var(--accent) 32%, transparent) !important;
  }
  :global(.cm-host .cm-focused .cm-selectionBackground) {
    background: color-mix(in srgb, var(--accent) 40%, transparent) !important;
  }
  :global(.cm-host ::selection) {
    background: color-mix(in srgb, var(--accent) 32%, transparent);
  }

  /* ================= Live Preview 语义样式 ================= */
  /* 标题：行装饰 cm-hd-N 控制字号/字重（阅读态与编辑态观感尽量一致） */
  :global(.cm-hd) {
    font-weight: 700;
    line-height: 1.3;
  }
  :global(.cm-hd-1) {
    font-size: 1.9em;
  }
  :global(.cm-hd-2) {
    font-size: 1.55em;
  }
  :global(.cm-hd-3) {
    font-size: 1.3em;
  }
  :global(.cm-hd-4) {
    font-size: 1.15em;
  }
  :global(.cm-hd-5) {
    font-size: 1.05em;
  }
  :global(.cm-hd-6) {
    font-size: 1em;
    color: var(--text-dim);
  }

  /* 行内格式 */
  :global(.cm-strong) {
    font-weight: 700;
  }
  :global(.cm-em) {
    font-style: italic;
  }
  :global(.cm-strike) {
    text-decoration: line-through;
    color: var(--text-dim);
  }
  :global(.cm-link) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: text;
  }
  /* 代码底色用半透明（而非不透明 --code-bg）：CM 的选区图层画在内容层下方，
     不透明的行/行内背景会挡住选区 → 选中代码时看不出被选中。半透明让选区透出。 */
  :global(.cm-inline-code) {
    background: color-mix(in srgb, var(--code-bg) 70%, transparent);
    color: var(--text);
    border-radius: 4px;
    padding: 0.1em 0.35em;
    font-size: 0.9em;
  }
  :global(.cm-highlight) {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    border-radius: 3px;
    padding: 0.05em 0.15em;
  }
  :global(.cm-lp-bullet) {
    color: var(--text-dim);
  }

  /* 引用：整行左边框 + 弱色（QuoteMark 已隐藏） */
  :global(.cm-blockquote) {
    border-left: 3px solid var(--border);
    padding-left: 14px;
    color: var(--text-dim);
  }

  /* 围栏代码块：整行底色（半透明，让选区透出，见上）+ 等宽（内容仍可编辑） */
  :global(.cm-code-block) {
    background: color-mix(in srgb, var(--code-bg) 60%, transparent);
  }
  :global(.cm-fence-info) {
    color: var(--text-dim);
    font-size: 0.85em;
  }

  /* 块级 widget */
  :global(.cm-lp-image) {
    display: inline-flex;
    flex-direction: column;
    gap: 4px;
    max-width: 100%;
    vertical-align: top;
  }
  :global(.cm-lp-image img) {
    max-width: 100%;
    max-height: 480px;
    border-radius: 6px;
  }
  :global(.cm-lp-image-cap) {
    font-size: 12px;
    color: var(--text-dim);
  }
  :global(.cm-lp-hr) {
    display: block;
    padding: 6px 0;
  }
  :global(.cm-lp-hr hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 0;
  }
  :global(.cm-lp-table) {
    padding: 4px 0;
    overflow-x: auto;
  }
  :global(.cm-lp-table table) {
    border-collapse: collapse;
  }
  :global(.cm-lp-table th),
  :global(.cm-lp-table td) {
    border: 1px solid var(--border);
    padding: 4px 10px;
  }
  :global(.cm-lp-table th) {
    background: var(--bg-side);
  }
  :global(.cm-lp-math-block) {
    display: block;
    text-align: center;
    padding: 6px 0;
  }
  :global(.cm-lp-task) {
    margin: 0 6px 0 0;
    vertical-align: middle;
    cursor: pointer;
    accent-color: var(--accent);
  }
</style>
