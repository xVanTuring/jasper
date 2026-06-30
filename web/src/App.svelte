<script lang="ts">
  import { onMount } from 'svelte'
  import { slide } from 'svelte/transition'
  import { api, IS_DEMO, type FolderNode, type NoteSummary, type NoteDetail } from './lib/api'
  import { t, getLocale, toggleLocale } from './lib/i18n.svelte'
  import Button from './lib/Button.svelte'
  import ThemePicker from './lib/ThemePicker.svelte'
  import FolderTree from './lib/FolderTree.svelte'
  import NoteList from './lib/NoteList.svelte'
  import NoteView from './lib/NoteView.svelte'
  import Settings from './lib/Settings.svelte'
  import ResourcePanel from './lib/ResourcePanel.svelte'

  // 让 <html lang> 跟随当前语言（影响断词/无障碍等）
  $effect(() => {
    document.documentElement.lang = getLocale()
  })


  let folders = $state<FolderNode[]>([])
  let selectedFolderId = $state<string | null>(null)
  let notes = $state<NoteSummary[]>([])
  let selectedNoteId = $state<string | null>(null)
  let detail = $state<NoteDetail | null>(null)
  let editOnOpenId = $state<string | null>(null)

  let query = $state('')
  let searchMode = $state(false)
  let listTitle = $state('')
  let error = $state('')

  let configured = $state<boolean | null>(null)
  let showSettings = $state(false)
  let showResources = $state(false)
  let showDemoBanner = $state(true)

  // 资源被删除/重命名后，刷新当前笔记详情（被引用资源变动可能影响渲染）
  async function onResourcesChanged() {
    if (selectedNoteId) {
      try {
        detail = await api.note(selectedNoteId)
      } catch {
        /* 忽略 */
      }
    }
  }

  // 记忆上次打开的笔记：仅存笔记 id，重载时按 id 拉取（失效则回退）。
  const LAST_NOTE_KEY = 'jasper.lastNote'
  function loadLastNoteId(): string | null {
    try {
      return localStorage.getItem(LAST_NOTE_KEY)
    } catch {
      return null
    }
  }
  function saveLastNoteId(id: string | null) {
    try {
      if (id) localStorage.setItem(LAST_NOTE_KEY, id)
      else localStorage.removeItem(LAST_NOTE_KEY)
    } catch {
      /* 忽略（隐私模式/storage 被禁用） */
    }
  }

  onMount(checkStatus)

  async function checkStatus() {
    try {
      const s = await api.status()
      configured = s.configured
      if (configured) await loadFolders()
    } catch (e) {
      error = `${e}`
    }
  }

  async function loadFolders() {
    try {
      folders = await api.folders()
      detail = null
      selectedNoteId = null
      // 优先恢复上次打开的笔记（含其所在笔记本）；无记录/已失效则回退到首个有笔记的笔记本
      if (await restoreLastNote()) return
      const first = findFirstWithNotes(folders)
      if (first) {
        selectFolder(first.id, first.title)
      } else {
        notes = []
        selectedFolderId = null
        listTitle = ''
      }
    } catch (e) {
      error = t('app.loadFailed', { err: `${e}` })
    }
  }

  // 恢复上次打开的笔记：拉取详情成功 → 选中其所在笔记本并打开；笔记已不存在 → 清除记忆并回退。
  async function restoreLastNote(): Promise<boolean> {
    const id = loadLastNoteId()
    if (!id) return false
    try {
      const d = await api.note(id)
      await selectFolder(d.parent_id)
      detail = d
      selectedNoteId = id
      return true
    } catch {
      saveLastNoteId(null)
      return false
    }
  }

  // 配置完成（首次或切换数据源）后重载
  async function onConfigured() {
    configured = true
    showSettings = false
    query = ''
    searchMode = false
    await loadFolders()
  }

  function findFirstWithNotes(list: FolderNode[]): FolderNode | null {
    for (const f of list) {
      if (f.note_count > 0) return f
      const child = findFirstWithNotes(f.children)
      if (child) return child
    }
    return list[0] ?? null
  }

  function findTitle(list: FolderNode[], id: string): string {
    for (const f of list) {
      if (f.id === id) return f.title
      const t = findTitle(f.children, id)
      if (t) return t
    }
    return ''
  }

  async function selectFolder(id: string, title?: string) {
    searchMode = false
    selectedFolderId = id
    listTitle = title ?? findTitle(folders, id) ?? ''
    try {
      notes = await api.notes(id)
    } catch (e) {
      error = `${e}`
    }
  }

  // 先取详情再切换 id（NoteView 按 id 重挂载，挂载时 detail 须已就绪）
  async function selectNote(id: string) {
    editOnOpenId = null
    try {
      const d = await api.note(id)
      detail = d
      selectedNoteId = id
      saveLastNoteId(id)
    } catch (e) {
      error = `${e}`
    }
  }

  // 笔记内部链接点击：是笔记则跳转，否则当资源在新标签打开
  async function navigate(id: string) {
    editOnOpenId = null
    try {
      const d = await api.note(id)
      detail = d
      selectedNoteId = id
      saveLastNoteId(id)
    } catch {
      window.open(api.resourceUrl(id), '_blank')
    }
  }

  // 重新拉取当前列表（保存后标题/时间/排序更新）
  async function refreshList() {
    try {
      if (searchMode) notes = await api.search(query.trim())
      else if (selectedFolderId != null) notes = await api.notes(selectedFolderId)
    } catch (e) {
      error = `${e}`
    }
  }

  async function handleNew() {
    const parent = searchMode ? '' : selectedFolderId ?? ''
    try {
      const n = await api.createNote({ parent_id: parent, title: t('note.newNote'), body: '' })
      editOnOpenId = n.id
      detail = n
      selectedNoteId = n.id
      saveLastNoteId(n.id)
      await refreshList()
    } catch (e) {
      error = `${e}`
    }
  }

  async function onNoteDeleted() {
    detail = null
    selectedNoteId = null
    editOnOpenId = null
    saveLastNoteId(null)
    await refreshList()
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined
  function onSearchInput() {
    clearTimeout(searchTimer)
    const q = query.trim()
    if (!q) {
      if (selectedFolderId) selectFolder(selectedFolderId)
      return
    }
    searchTimer = setTimeout(async () => {
      try {
        notes = await api.search(q)
        searchMode = true
        listTitle = t('list.searchPrefix', { q })
      } catch (e) {
        error = `${e}`
      }
    }, 200)
  }
</script>

<div class="app">
  <header class="topbar">
    <div class="brand">Jasper</div>
    <input
      class="search"
      type="search"
      placeholder={t('topbar.search')}
      bind:value={query}
      oninput={onSearchInput}
    />
    <div class="topbar-actions">
      <Button
        variant="default"
        label={getLocale() === 'zh' ? '中' : 'EN'}
        title={t('common.langTitle')}
        onclick={toggleLocale}
      />
      <ThemePicker />
      {#if !IS_DEMO}
        <Button variant="ghost" iconOnly icon="image" label={t('topbar.resources')} onclick={() => (showResources = true)} />
        <Button variant="ghost" iconOnly icon="settings" label={t('topbar.settings')} onclick={() => (showSettings = true)} />
      {/if}
    </div>
  </header>

  {#if IS_DEMO && showDemoBanner}
    <div class="demo-banner" transition:slide={{ duration: 200 }}>
      <span class="msg">
        {@html t('demo.banner')}
        <span class="dim">{t('demo.bannerDim')}</span>
      </span>
      <Button variant="ghost" iconOnly icon="close" label={t('common.close')} onclick={() => (showDemoBanner = false)} />
    </div>
  {/if}

  {#if error}
    <div class="error" transition:slide={{ duration: 200 }}>{error}</div>
  {/if}

  <div class="panes">
    <aside class="sidebar">
      <div class="pane-title">{t('pane.notebooks')}</div>
      <FolderTree
        {folders}
        selectedId={searchMode ? null : selectedFolderId}
        onSelect={(id) => selectFolder(id)}
      />
    </aside>

    <section class="notelist">
      <div class="pane-title">
        <span>{listTitle}</span>
        {#if !IS_DEMO}
          <Button variant="ghost" iconOnly icon="plus" label={t('pane.newNote')} onclick={handleNew} />
        {/if}
      </div>
      <NoteList {notes} selectedId={selectedNoteId} onSelect={selectNote} />
    </section>

    <main class="reader">
      {#key selectedNoteId}
        <NoteView
          {detail}
          onNavigate={navigate}
          onChanged={refreshList}
          onDeleted={onNoteDeleted}
          initialEdit={detail != null && detail.id === editOnOpenId}
          readOnly={IS_DEMO}
        />
      {/key}
    </main>
  </div>
</div>

{#if configured === false}
  <Settings mode="setup" onDone={onConfigured} />
{:else if showSettings}
  <Settings mode="settings" onDone={onConfigured} onClose={() => (showSettings = false)} />
{/if}

{#if showResources}
  <ResourcePanel onClose={() => (showResources = false)} onChanged={onResourcesChanged} />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 14px;
    height: 44px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-bar);
    flex: 0 0 auto;
  }
  .brand {
    font-weight: 600;
    font-size: 14px;
    color: var(--accent);
  }
  .search {
    flex: 1;
    max-width: 420px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 0 10px;
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .search:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .topbar-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .error {
    background: var(--danger);
    color: var(--on-accent);
    padding: 6px 14px;
    font-size: 13px;
  }
  .demo-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 14px;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text);
    background: var(--accent-soft);
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .demo-banner .dim {
    color: var(--text-dim);
  }
  .demo-banner .msg {
    flex: 1;
  }
  .panes {
    display: grid;
    grid-template-columns: 250px 300px 1fr;
    flex: 1;
    min-height: 0;
  }
  .sidebar {
    border-right: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0 0 12px;
    background: var(--bg-side);
  }
  .notelist {
    border-right: 1px solid var(--border);
    overflow-y: auto;
    background: var(--bg-side);
  }
  .pane-title {
    position: sticky;
    top: 0;
    z-index: 10;
    height: 38px;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    box-sizing: border-box;
    background: var(--bg-side);
    padding: 0 6px 0 12px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
  }
  .reader {
    overflow-y: auto;
    background: var(--bg);
  }
</style>
