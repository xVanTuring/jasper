<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type ResourceInfo } from './api'

  let {
    onClose,
    onChanged,
  }: {
    onClose: () => void
    onChanged?: () => void // 删除/重命名后通知父级刷新（被引用资源变动可能影响当前笔记显示）
  } = $props()

  let items = $state<ResourceInfo[]>([])
  let loading = $state(true)
  let error = $state('')
  let working = $state(false)

  let editingId = $state<string | null>(null)
  let editTitle = $state('')

  let orphanCount = $derived(items.filter((r) => r.used_by === 0).length)
  let totalSize = $derived(items.reduce((s, r) => s + r.size, 0))

  onMount(load)

  async function load() {
    loading = true
    error = ''
    try {
      items = await api.resources()
    } catch (e) {
      error = `${e}`
    } finally {
      loading = false
    }
  }

  function fmtSize(n: number): string {
    if (n < 1024) return `${n} B`
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
    return `${(n / 1024 / 1024).toFixed(1)} MB`
  }

  function isImage(mime: string) {
    return mime.startsWith('image/')
  }

  function startRename(r: ResourceInfo) {
    editingId = r.id
    editTitle = r.title
  }
  function cancelRename() {
    editingId = null
    editTitle = ''
  }
  async function saveRename(r: ResourceInfo) {
    const t = editTitle.trim()
    if (!t || t === r.title) {
      cancelRename()
      return
    }
    working = true
    try {
      await api.renameResource(r.id, t)
      cancelRename()
      await load()
      onChanged?.()
    } catch (e) {
      error = `${e}`
    } finally {
      working = false
    }
  }

  async function removeOne(r: ResourceInfo) {
    const warn =
      r.used_by > 0
        ? `资源「${r.title}」被 ${r.used_by} 篇笔记引用，删除后这些笔记的图片/附件会失效。仍要删除？`
        : `删除未被引用的资源「${r.title}」？`
    if (!confirm(warn)) return
    working = true
    try {
      await api.deleteResource(r.id)
      await load()
      onChanged?.()
    } catch (e) {
      error = `${e}`
    } finally {
      working = false
    }
  }

  // 重命名输入框挂载即聚焦并全选（替代 autofocus，避开 a11y 告警）
  function focusInput(node: HTMLInputElement) {
    node.focus()
    node.select()
  }

  async function cleanupOrphans() {
    const orphans = items.filter((r) => r.used_by === 0)
    if (!orphans.length) return
    if (!confirm(`将删除 ${orphans.length} 个未被任何笔记引用的资源，无法撤销。继续？`)) return
    working = true
    try {
      for (const r of orphans) await api.deleteResource(r.id)
      await load()
      onChanged?.()
    } catch (e) {
      error = `${e}`
    } finally {
      working = false
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onClose()} />

<!-- 点击遮罩空白处关闭（仅当点中遮罩本身，不含卡片内部） -->
<div class="overlay" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="card">
    <header>
      <h2>资源管理</h2>
      <button class="x" onclick={onClose} aria-label="关闭">✕</button>
    </header>

    <div class="bar">
      <span class="stat">
        {items.length} 个资源 · {fmtSize(totalSize)}
        {#if orphanCount > 0}· <span class="orphan-stat">{orphanCount} 个孤儿</span>{/if}
      </span>
      <button class="cleanup" onclick={cleanupOrphans} disabled={working || orphanCount === 0}>
        🧹 清理孤儿 ({orphanCount})
      </button>
    </div>

    {#if error}<div class="error">⚠️ {error}</div>{/if}

    {#if loading}
      <div class="empty">加载中…</div>
    {:else if items.length === 0}
      <div class="empty">还没有资源。在编辑笔记时粘贴/拖拽图片或用「📎 附件」上传。</div>
    {:else}
      <ul class="list">
        {#each items as r (r.id)}
          <li class="row" class:orphan={r.used_by === 0}>
            <a class="thumb" href={api.resourceUrl(r.id)} target="_blank" rel="noopener" title="在新标签打开">
              {#if isImage(r.mime)}
                <img src={api.resourceUrl(r.id)} alt={r.title} loading="lazy" />
              {:else}
                <span class="ext">{r.file_extension || '?'}</span>
              {/if}
            </a>
            <div class="info">
              {#if editingId === r.id}
                <input
                  class="rename"
                  bind:value={editTitle}
                  onkeydown={(e) => {
                    if (e.key === 'Enter') {
                      e.stopPropagation()
                      saveRename(r)
                    } else if (e.key === 'Escape') {
                      e.stopPropagation() // 不冒泡到 window，避免连带关闭面板
                      cancelRename()
                    }
                  }}
                  use:focusInput
                />
              {:else}
                <div class="title" title={r.title}>{r.title || '(无标题)'}</div>
              {/if}
              <div class="meta">
                <span>{r.mime || '未知类型'}</span>
                <span>·</span>
                <span>{fmtSize(r.size)}</span>
                <span>·</span>
                {#if r.used_by > 0}
                  <span class="used">被 {r.used_by} 篇引用</span>
                {:else}
                  <span class="badge">未被引用</span>
                {/if}
              </div>
            </div>
            <div class="actions">
              {#if editingId === r.id}
                <button class="mini" onclick={() => saveRename(r)} disabled={working}>保存</button>
                <button class="mini" onclick={cancelRename}>取消</button>
              {:else}
                <button class="mini" onclick={() => startRename(r)} disabled={working} title="重命名">✎</button>
                <button class="mini danger" onclick={() => removeOne(r)} disabled={working} title="删除">🗑</button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .card {
    width: 640px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 18px 20px 16px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .x {
    background: none;
    border: none;
    font-size: 16px;
    color: var(--text-dim);
    cursor: pointer;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 12px 0 6px;
  }
  .stat {
    font-size: 12px;
    color: var(--text-dim);
  }
  .orphan-stat {
    color: #c0392b;
  }
  .cleanup {
    background: none;
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 5px 10px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }
  .cleanup:hover:not(:disabled) {
    background: var(--hover);
  }
  .cleanup:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .error {
    margin: 8px 0;
    padding: 8px 10px;
    background: rgba(192, 57, 43, 0.12);
    color: #c0392b;
    border-radius: 7px;
    font-size: 12px;
    word-break: break-all;
  }
  .empty {
    padding: 40px 0;
    text-align: center;
    color: var(--text-dim);
    font-size: 13px;
  }
  .list {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 4px;
    border-bottom: 1px solid var(--border);
  }
  .row.orphan {
    background: rgba(192, 57, 43, 0.05);
  }
  .thumb {
    flex: 0 0 auto;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-side);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    text-decoration: none;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .ext {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rename {
    width: 100%;
    box-sizing: border-box;
    padding: 4px 7px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--bg-side);
    color: var(--text);
    font-size: 13px;
  }
  .meta {
    display: flex;
    gap: 5px;
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 3px;
  }
  .used {
    color: var(--text-dim);
  }
  .badge {
    color: #c0392b;
  }
  .actions {
    flex: 0 0 auto;
    display: flex;
    gap: 4px;
  }
  .mini {
    background: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    background: var(--hover);
  }
  .mini.danger:hover:not(:disabled) {
    background: #c0392b;
    color: #fff;
    border-color: #c0392b;
  }
  .mini:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
