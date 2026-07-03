<script lang="ts">
  import { flip } from 'svelte/animate'
  import type { TagInfo } from './api'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'

  let {
    tags,
    selectedId,
    onSelect,
  }: {
    tags: TagInfo[]
    selectedId: string | null
    onSelect: (id: string, title: string) => void
  } = $props()
</script>

{#if tags.length}
  <div class="tag-section">
    <div class="tag-head">{t('pane.tags')}</div>
    <ul class="taglist">
      {#each tags as tag (tag.id)}
        <li animate:flip={{ duration: 180 }}>
          <button
            class="row"
            class:active={tag.id === selectedId}
            onclick={() => onSelect(tag.id, tag.title)}
          >
            <span class="ic"><Icon name="tag" size={13} /></span>
            <span class="name">{tag.title || t('common.unnamed')}</span>
            {#if tag.note_count > 0}<span class="count">{tag.note_count}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .tag-section {
    border-top: 1px solid var(--border);
    margin-top: 6px;
    padding-top: 4px;
  }
  .tag-head {
    padding: 6px 12px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-dim);
    text-transform: uppercase;
  }
  .taglist {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
    width: calc(100% - 8px);
    height: 30px;
    margin: 1px 4px;
    padding: 0 8px 0 6px;
    border: none;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    text-align: left;
    border-radius: 7px;
    cursor: pointer;
    overflow: hidden;
    transition: background 0.13s ease;
  }
  .row::before {
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
  .row:hover {
    background: var(--hover);
  }
  .row.active {
    background: var(--accent-soft);
  }
  .row.active::before {
    height: 62%;
  }
  .ic {
    opacity: 0.85;
    flex: 0 0 auto;
    color: var(--text-dim);
  }
  .row.active .ic {
    color: var(--accent);
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.active .name {
    color: var(--accent);
    font-weight: 600;
  }
  .count {
    flex: 0 0 auto;
    margin-left: 6px;
    color: var(--text-dim);
    font-size: 11px;
    background: var(--hover);
    border-radius: 9px;
    padding: 1px 7px;
    min-width: 20px;
    text-align: center;
  }
  .row.active .count {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    color: var(--accent);
  }
</style>
