<script lang="ts">
  import { fly } from 'svelte/transition'
  import { flip } from 'svelte/animate'
  import { cubicOut } from 'svelte/easing'
  import { NOTE_DND_TYPE, type NoteSummary } from './api'
  import { t } from './i18n.svelte'

  let {
    notes,
    selectedId,
    onSelect,
    canDrag = false,
  }: {
    notes: NoteSummary[]
    selectedId: string | null
    onSelect: (id: string) => void
    canDrag?: boolean
  } = $props()

  // 拖拽载荷用自定义 MIME（NOTE_DND_TYPE，见 api.ts），FolderTree 据此识别为“笔记拖拽”。
  function onDragStart(e: DragEvent, id: string) {
    if (!e.dataTransfer) return
    e.dataTransfer.setData(NOTE_DND_TYPE, id)
    e.dataTransfer.effectAllowed = 'move'
  }

  function fmtDate(ms: number): string {
    if (!ms) return ''
    const d = new Date(ms)
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }
</script>

<ul class="list">
  {#each notes as n, i (n.id)}
    <li
      in:fly|global={{ y: 6, duration: 220, delay: Math.min(i * 18, 260), easing: cubicOut }}
      animate:flip={{ duration: 220, easing: cubicOut }}
    >
      <button
        class="note"
        class:active={n.id === selectedId}
        draggable={canDrag}
        ondragstart={canDrag ? (e) => onDragStart(e, n.id) : undefined}
        onclick={() => onSelect(n.id)}
      >
        <div class="line1">
          {#if n.is_todo}
            <input class="todo" type="checkbox" checked={n.todo_completed} disabled />
          {/if}
          <span class="title" class:done={n.is_todo && n.todo_completed}>
            {n.title || t('common.untitled')}
          </span>
        </div>
        <div class="meta">
          <span class="date">{fmtDate(n.updated_time)}</span>
          {#if n.task_total > 0}
            <span
              class="tasks"
              class:done={n.task_done === n.task_total}
              title={t('list.tasks', { done: n.task_done, total: n.task_total })}
            >
              <span class="bar">
                <span class="fill" style="width:{Math.round((n.task_done / n.task_total) * 100)}%"></span>
              </span>
              {n.task_done}/{n.task_total}
            </span>
          {/if}
        </div>
      </button>
    </li>
  {/each}
  {#if notes.length === 0}
    <li class="empty">{t('list.empty')}</li>
  {/if}
</ul>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .note {
    position: relative;
    width: 100%;
    display: block;
    text-align: left;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    padding: 9px 12px;
    cursor: pointer;
    color: var(--text);
    transition: background 0.13s ease;
  }
  .note::before {
    content: '';
    position: absolute;
    left: 0;
    top: 50%;
    width: 3px;
    height: 0;
    background: var(--accent);
    border-radius: 0 2px 2px 0;
    transform: translateY(-50%);
    transition: height 0.18s ease;
  }
  .note.active {
    background: var(--accent-soft);
  }
  .note.active::before {
    height: 64%;
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
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: 3px;
  }
  .date {
    font-size: 11px;
    color: var(--text-dim);
  }
  .tasks {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: 0 0 auto;
    font-size: 11px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .tasks .bar {
    width: 34px;
    height: 4px;
    border-radius: 2px;
    background: var(--hover);
    overflow: hidden;
  }
  .tasks .fill {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .tasks.done {
    color: var(--success);
  }
  .tasks.done .fill {
    background: var(--success);
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
