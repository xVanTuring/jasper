<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { api } from './api'
  import { t } from './i18n.svelte'
  import { resolvedTheme } from './theme.svelte'
  import Button from './Button.svelte'

  let {
    value,
    onChange,
  }: {
    value: string
    onChange: (v: string) => void
  } = $props()

  let host: HTMLDivElement
  let fileInput: HTMLInputElement
  let view: import('@codemirror/view').EditorView | undefined

  let uploading = $state(0)
  let uploadErr = $state('')

  // 在光标处插入文本（替换选区），插入后聚焦并把光标移到末尾。
  function insertAtCursor(text: string) {
    if (!view) return
    const sel = view.state.selection.main
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: text },
      selection: { anchor: sel.from + text.length },
    })
    view.focus()
  }

  // 给无名 Blob（如截图粘贴）造一个带扩展名的文件名，便于资源标题/扩展名识别。
  function nameOf(file: File): string {
    if (file.name) return file.name
    const ext = (file.type.split('/')[1] || 'bin').replace('+xml', '')
    return `pasted-${Date.now()}.${ext}`
  }

  async function uploadFiles(files: File[]) {
    if (!files.length) return
    uploadErr = ''
    for (const file of files) {
      uploading++
      try {
        const r = await api.uploadResource(file, nameOf(file))
        insertAtCursor(r.markdown + '\n')
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
    fileInput.value = '' // 允许重复选同一文件
  }

  onMount(async () => {
    // 懒加载 CodeMirror —— 仅在进入编辑模式时才下载这部分代码
    const [{ EditorView, basicSetup }, { markdown }, { oneDark }] = await Promise.all([
      import('codemirror'),
      import('@codemirror/lang-markdown'),
      import('@codemirror/theme-one-dark'),
    ])
    const dark = resolvedTheme() === 'dark'
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        basicSetup,
        markdown(),
        EditorView.lineWrapping,
        ...(dark ? [oneDark] : []),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChange(u.state.doc.toString())
        }),
        // 粘贴/拖拽文件即上传为资源；只有确实含文件时才拦截默认行为
        EditorView.domEventHandlers({
          paste: (e) => {
            const files = filesFrom(e.clipboardData)
            if (!files.length) return false
            e.preventDefault()
            uploadFiles(files)
            return true
          },
          drop: (e) => {
            const files = filesFrom(e.dataTransfer)
            if (!files.length) return false
            e.preventDefault()
            uploadFiles(files)
            return true
          },
        }),
      ],
    })
    view.focus()
  })

  function filesFrom(dt: DataTransfer | null): File[] {
    if (!dt) return []
    if (dt.files && dt.files.length) return Array.from(dt.files)
    // 截图粘贴：文件在 items 里（kind==='file'）
    const out: File[] = []
    for (const it of Array.from(dt.items || [])) {
      if (it.kind === 'file') {
        const f = it.getAsFile()
        if (f) out.push(f)
      }
    }
    return out
  }

  onDestroy(() => view?.destroy())
</script>

<div class="editor-col">
  <div class="ed-toolbar">
    <Button variant="default" icon="attach" label={t('editor.attach')} onclick={pickFile} disabled={uploading > 0} />
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
  .hint {
    font-size: 12px;
    color: var(--text-dim);
  }
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
  :global(.cm-editor) {
    height: 100%;
  }
  :global(.cm-scroller) {
    font-family: 'SF Mono', Menlo, Consolas, monospace;
    font-size: 13px;
    line-height: 1.6;
  }
  :global(.cm-editor.cm-focused) {
    outline: none;
  }
</style>
