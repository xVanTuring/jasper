<script lang="ts">
  import { api, type TagRef } from './api'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'

  let {
    noteId,
    readOnly = false,
    suggestions = [],
    onChanged,
    refreshToken = 0,
  }: {
    noteId: string
    readOnly?: boolean
    suggestions?: string[]
    onChanged?: () => void
    refreshToken?: number
  } = $props()

  let tags = $state<TagRef[]>([])
  let input = $state('')
  let busy = $state(false)
  let error = $state('')

  // 按 noteId / 外部刷新令牌重新拉取（组件本就随笔记 id 重挂载；令牌覆盖 SSE 外部变更）
  $effect(() => {
    const id = noteId
    void refreshToken // 追踪：变化即重载
    let cancelled = false
    api
      .noteTags(id)
      .then((t) => {
        if (!cancelled) tags = t
      })
      .catch(() => {
        /* 忽略：下次操作/刷新会纠正 */
      })
    return () => {
      cancelled = true
    }
  })

  async function add() {
    const title = input.trim()
    if (!title || busy) return
    busy = true
    error = ''
    try {
      tags = await api.addNoteTag(noteId, title)
      input = ''
      onChanged?.()
    } catch (e) {
      error = e instanceof Error ? e.message : t('tags.actionFailed')
    } finally {
      busy = false
    }
  }

  async function remove(tagId: string) {
    if (busy) return
    busy = true
    error = ''
    try {
      tags = await api.removeNoteTag(noteId, tagId)
      onChanged?.()
    } catch (e) {
      error = e instanceof Error ? e.message : t('tags.actionFailed')
    } finally {
      busy = false
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault()
      add()
    } else if (e.key === 'Escape') {
      input = ''
    }
  }
</script>

{#if !readOnly || tags.length}
  <div class="note-tags">
    <span class="lead" title={t('pane.tags')}><Icon name="tag" size={12} /></span>
    {#each tags as tag (tag.id)}
      <span class="chip">
        <span class="chip-label">{tag.title}</span>
        {#if !readOnly}
          <button
            class="chip-x"
            title={t('tags.remove')}
            aria-label={t('tags.remove')}
            onclick={() => remove(tag.id)}
            disabled={busy}
          >
            <Icon name="close" size={10} />
          </button>
        {/if}
      </span>
    {/each}
    {#if !readOnly}
      <input
        class="add"
        list="tag-suggest-{noteId}"
        bind:value={input}
        placeholder={t('tags.add')}
        onkeydown={onKey}
        disabled={busy}
      />
      <datalist id="tag-suggest-{noteId}">
        {#each suggestions as s}<option value={s}></option>{/each}
      </datalist>
    {/if}
    {#if error}<span class="err" title={error}>{t('tags.actionFailed')}</span>{/if}
  </div>
{/if}

<style>
  .note-tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 36px;
    border-bottom: 1px solid var(--border);
  }
  .lead {
    display: inline-flex;
    align-items: center;
    color: var(--text-dim);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    max-width: 220px;
    padding: 2px 4px 2px 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-side);
    font-size: 12px;
    color: var(--text);
  }
  .chip-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: none;
    color: var(--text-dim);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .chip-x:hover:not(:disabled) {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .chip-x:disabled {
    cursor: default;
    opacity: 0.5;
  }
  .add {
    flex: 0 1 140px;
    min-width: 90px;
    height: 22px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: none;
    color: var(--text);
    font-size: 12px;
    transition: border-color 0.15s ease, background 0.15s ease;
  }
  .add::placeholder {
    color: var(--text-dim);
  }
  .add:hover {
    border-color: var(--border);
  }
  .add:focus {
    outline: none;
    border-color: var(--accent);
    background: var(--bg);
  }
  .err {
    font-size: 12px;
    color: var(--danger);
  }
</style>
