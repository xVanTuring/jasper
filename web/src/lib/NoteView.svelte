<script lang="ts">
  import { onDestroy } from 'svelte'
  import type { NoteDetail } from './api'
  import { api } from './api'
  import { renderNote } from './render'
  import Editor from './Editor.svelte'

  let {
    detail,
    onNavigate,
    onChanged,
    onDeleted,
    initialEdit = false,
    readOnly = false,
  }: {
    detail: NoteDetail | null
    onNavigate: (id: string) => void
    onChanged: () => void
    onDeleted: () => void
    initialEdit?: boolean
    readOnly?: boolean
  } = $props()

  // 本组件按笔记 id 在父级以 {#key} 重挂载，故这里用初始值即可，无需响应 detail 变化。
  let editMode = $state(initialEdit)
  let title = $state(detail?.title ?? '')
  let body = $state(detail?.body ?? '')

  let dirty = false
  let saveState = $state<'idle' | 'saving' | 'saved' | 'error'>('idle')
  let timer: ReturnType<typeof setTimeout> | undefined

  // 阅读视图按本地（可能已编辑）的标题/正文渲染，保证切回阅读即时反映改动
  let html = $derived(
    detail ? renderNote({ ...detail, title, body }) : ''
  )

  function scheduleSave() {
    if (!detail) return
    dirty = true
    saveState = 'saving'
    clearTimeout(timer)
    timer = setTimeout(save, 800)
  }

  async function save() {
    if (!detail || !dirty) return
    clearTimeout(timer)
    const id = detail.id
    try {
      await api.updateNote(id, { title, body })
      dirty = false
      saveState = 'saved'
      onChanged()
    } catch {
      saveState = 'error'
    }
  }

  function onBodyChange(v: string) {
    body = v
    scheduleSave()
  }

  async function remove() {
    if (!detail) return
    if (!confirm(`确定删除「${title || '无标题'}」？`)) return
    try {
      await api.deleteNote(detail.id)
      onDeleted()
    } catch {
      saveState = 'error'
    }
  }

  // 切换笔记/卸载时，若有未保存改动则立即冲刷
  onDestroy(() => {
    if (dirty && detail) {
      clearTimeout(timer)
      const id = detail.id
      api.updateNote(id, { title, body }).then(onChanged).catch(() => {})
    }
  })

  function fmtDateTime(ms: number): string {
    if (!ms) return ''
    return new Date(ms).toLocaleString()
  }

  function onContentClick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest('[data-internal-id]') as HTMLElement | null
    if (!el) return
    e.preventDefault()
    const id = el.getAttribute('data-internal-id')
    if (id) onNavigate(id)
  }
</script>

{#if detail}
  <article class="note-view">
    <div class="toolbar">
      <div class="left">
        {#if editMode}
          <span class="save-state {saveState}">
            {saveState === 'saving' ? '保存中…' : saveState === 'saved' ? '已保存' : saveState === 'error' ? '保存失败' : ''}
          </span>
        {/if}
      </div>
      <div class="right">
        {#if !readOnly}
          <button class="btn" onclick={() => (editMode = !editMode)}>
            {editMode ? '👁 阅读' : '✏️ 编辑'}
          </button>
          <button class="btn danger" onclick={remove}>🗑 删除</button>
        {/if}
      </div>
    </div>

    {#if editMode}
      <input
        class="title-input"
        bind:value={title}
        oninput={scheduleSave}
        placeholder="标题"
      />
      <div class="editor-wrap">
        <Editor value={body} onChange={onBodyChange} />
      </div>
    {:else}
      <h1 class="note-title">{title || '(无标题)'}</h1>
      <div class="meta">
        更新于 {fmtDateTime(detail.updated_time)}
        {#if detail.source_url}
          · <a href={detail.source_url} target="_blank" rel="noopener noreferrer">来源</a>
        {/if}
        {#if detail.markup_language === 2}<span class="badge">HTML</span>{/if}
      </div>
      <!-- 内容已由 DOMPurify 净化 -->
      <div class="content" onclick={onContentClick}>{@html html}</div>
    {/if}
  </article>
{:else}
  <div class="placeholder">选择一篇笔记查看</div>
{/if}

<style>
  .note-view {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .save-state {
    font-size: 12px;
    color: var(--text-dim);
  }
  .save-state.saved {
    color: #2e7d32;
  }
  .save-state.error {
    color: #c0392b;
  }
  .btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    margin-left: 6px;
  }
  .btn:hover {
    background: var(--hover);
  }
  .btn.danger:hover {
    background: #c0392b;
    color: #fff;
    border-color: #c0392b;
  }
  .title-input {
    border: none;
    outline: none;
    background: none;
    color: var(--text);
    font-size: 22px;
    font-weight: 700;
    padding: 18px 36px 8px;
    flex: 0 0 auto;
  }
  .editor-wrap {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 0 20px 20px;
  }
  .note-title {
    font-size: 24px;
    margin: 0;
    padding: 24px 36px 6px;
  }
  .meta {
    font-size: 12px;
    color: var(--text-dim);
    padding: 0 36px 14px;
    border-bottom: 1px solid var(--border);
  }
  .badge {
    background: var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    margin-left: 6px;
  }
  .content {
    padding: 20px 36px 80px;
    max-width: 820px;
    overflow-y: auto;
  }
  .placeholder {
    display: flex;
    height: 100%;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
  }
</style>
