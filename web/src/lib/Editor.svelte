<script lang="ts">
  import { onMount, onDestroy } from 'svelte'

  let {
    value,
    onChange,
  }: {
    value: string
    onChange: (v: string) => void
  } = $props()

  let host: HTMLDivElement
  let view: import('@codemirror/view').EditorView | undefined

  onMount(async () => {
    // 懒加载 CodeMirror —— 仅在进入编辑模式时才下载这部分代码
    const [{ EditorView, basicSetup }, { markdown }, { oneDark }] = await Promise.all([
      import('codemirror'),
      import('@codemirror/lang-markdown'),
      import('@codemirror/theme-one-dark'),
    ])
    const dark = matchMedia('(prefers-color-scheme: dark)').matches
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
      ],
    })
    view.focus()
  })

  onDestroy(() => view?.destroy())
</script>

<div class="cm-host" bind:this={host}></div>

<style>
  .cm-host {
    height: 100%;
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
