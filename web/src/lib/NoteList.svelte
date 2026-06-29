<script lang="ts">
  import type { NoteSummary } from './api'

  let {
    notes,
    selectedId,
    onSelect,
  }: {
    notes: NoteSummary[]
    selectedId: string | null
    onSelect: (id: string) => void
  } = $props()

  function fmtDate(ms: number): string {
    if (!ms) return ''
    const d = new Date(ms)
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }
</script>

<ul class="list">
  {#each notes as n (n.id)}
    <li>
      <button class="note" class:active={n.id === selectedId} onclick={() => onSelect(n.id)}>
        <div class="line1">
          {#if n.is_todo}
            <input class="todo" type="checkbox" checked={n.todo_completed} disabled />
          {/if}
          <span class="title" class:done={n.is_todo && n.todo_completed}>
            {n.title || '(无标题)'}
          </span>
        </div>
        <div class="date">{fmtDate(n.updated_time)}</div>
      </button>
    </li>
  {/each}
  {#if notes.length === 0}
    <li class="empty">没有笔记</li>
  {/if}
</ul>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .note {
    width: 100%;
    display: block;
    text-align: left;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    padding: 9px 12px;
    cursor: pointer;
    color: var(--text);
  }
  .note.active {
    background: var(--accent-soft);
  }
  .note:hover:not(.active) {
    background: var(--hover);
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .title {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title.done {
    text-decoration: line-through;
    color: var(--text-dim);
  }
  .date {
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .todo {
    margin: 0;
  }
  .empty {
    padding: 20px 12px;
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
