<script lang="ts">
  import { slide } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import type { FolderNode } from './api'
  import { t } from './i18n.svelte'
  import Icon from './Icon.svelte'
  import Self from './FolderTree.svelte'

  let {
    folders,
    selectedId,
    onSelect,
    depth = 0,
  }: {
    folders: FolderNode[]
    selectedId: string | null
    onSelect: (id: string) => void
    depth?: number
  } = $props()

  // 默认折叠：只有展开过的笔记本 expanded[id] 为 true
  let expanded = $state<Record<string, boolean>>({})
</script>

<ul class="tree">
  {#each folders as f (f.id)}
    <li>
      <div
        class="row"
        class:active={f.id === selectedId}
        style="padding-left: {depth * 14 + 6}px"
      >
        {#if f.children.length}
          <button
            class="caret"
            class:open={expanded[f.id]}
            onclick={(e) => {
              e.stopPropagation()
              expanded[f.id] = !expanded[f.id]
            }}
            aria-label={t('tree.toggle')}
          >
            <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
              <path d="M3.5 1.5 L7.5 5 L3.5 8.5 Z" fill="currentColor" />
            </svg>
          </button>
        {:else}
          <span class="caret spacer"></span>
        {/if}

        <button class="folder" onclick={() => onSelect(f.id)}>
          <span class="ic"><Icon name={f.children.length ? 'folder' : 'file'} size={14} /></span>
          <span class="name">{f.title || t('common.unnamed')}</span>
          {#if f.note_count > 0}<span class="count">{f.note_count}</span>{/if}
        </button>
      </div>

      {#if f.children.length && expanded[f.id]}
        <div class="subtree" transition:slide={{ duration: 180, easing: cubicOut }}>
          <Self folders={f.children} {selectedId} {onSelect} depth={depth + 1} />
        </div>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    position: relative;
    display: flex;
    align-items: center;
    height: 30px;
    margin: 1px 4px;
    border-radius: 7px;
    cursor: pointer;
    transition: background 0.13s ease;
    overflow: hidden;
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
  .caret {
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 0;
  }
  .caret svg {
    transition: transform 0.13s ease;
  }
  .caret.open svg {
    transform: rotate(90deg);
  }
  .caret:hover {
    color: var(--text);
  }
  .caret.spacer {
    cursor: default;
  }
  .folder {
    flex: 1 1 auto;
    display: flex;
    align-items: center;
    gap: 7px;
    background: none;
    border: none;
    color: var(--text);
    text-align: left;
    padding: 0 8px 0 2px;
    height: 100%;
    cursor: pointer;
    font-size: 13px;
    min-width: 0;
  }
  .ic {
    font-size: 12px;
    opacity: 0.85;
    flex: 0 0 auto;
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
