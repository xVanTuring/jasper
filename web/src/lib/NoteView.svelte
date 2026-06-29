<script lang="ts">
  import type { NoteDetail } from './api'
  import { renderNote } from './render'

  let {
    detail,
    onNavigate,
  }: {
    detail: NoteDetail | null
    onNavigate: (id: string) => void
  } = $props()

  let html = $derived(detail ? renderNote(detail) : '')

  function fmtDateTime(ms: number): string {
    if (!ms) return ''
    return new Date(ms).toLocaleString()
  }

  // 处理笔记内部链接 :/id 的点击（笔记跳转 / 资源打开）
  function onClick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest('[data-internal-id]') as HTMLElement | null
    if (!el) return
    e.preventDefault()
    const id = el.getAttribute('data-internal-id')
    if (id) onNavigate(id)
  }
</script>

{#if detail}
  <article class="note-view">
    <h1 class="note-title">{detail.title || '(无标题)'}</h1>
    <div class="meta">
      更新于 {fmtDateTime(detail.updated_time)}
      {#if detail.source_url}
        · <a href={detail.source_url} target="_blank" rel="noopener noreferrer">来源</a>
      {/if}
      {#if detail.markup_language === 2}<span class="badge">HTML</span>{/if}
    </div>
    <!-- 内容已由 DOMPurify 净化 -->
    <div class="content" onclick={onClick}>{@html html}</div>
  </article>
{:else}
  <div class="placeholder">选择一篇笔记查看</div>
{/if}

<style>
  .note-view {
    max-width: 820px;
    margin: 0 auto;
    padding: 28px 36px 80px;
  }
  .note-title {
    font-size: 24px;
    margin: 0 0 6px;
  }
  .meta {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 20px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--border);
  }
  .badge {
    background: var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    margin-left: 6px;
  }
  .placeholder {
    display: flex;
    height: 100%;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
  }
</style>
