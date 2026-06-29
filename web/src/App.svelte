<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type FolderNode, type NoteSummary, type NoteDetail } from './lib/api'
  import FolderTree from './lib/FolderTree.svelte'
  import NoteList from './lib/NoteList.svelte'
  import NoteView from './lib/NoteView.svelte'

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

  onMount(async () => {
    try {
      folders = await api.folders()
      const first = findFirstWithNotes(folders)
      if (first) selectFolder(first.id, first.title)
    } catch (e) {
      error = `加载失败：${e}`
    }
  })

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
      const n = await api.createNote({ parent_id: parent, title: '新笔记', body: '' })
      editOnOpenId = n.id
      detail = n
      selectedNoteId = n.id
      await refreshList()
    } catch (e) {
      error = `${e}`
    }
  }

  async function onNoteDeleted() {
    detail = null
    selectedNoteId = null
    editOnOpenId = null
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
        listTitle = `搜索：${q}`
      } catch (e) {
        error = `${e}`
      }
    }, 200)
  }
</script>

<div class="app">
  <header class="topbar">
    <div class="brand">joplin-lite</div>
    <input
      class="search"
      type="search"
      placeholder="搜索笔记…"
      bind:value={query}
      oninput={onSearchInput}
    />
  </header>

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <div class="panes">
    <aside class="sidebar">
      <div class="pane-title">笔记本</div>
      <FolderTree
        {folders}
        selectedId={searchMode ? null : selectedFolderId}
        onSelect={(id) => selectFolder(id)}
      />
    </aside>

    <section class="notelist">
      <div class="pane-title">
        <span>{listTitle}</span>
        <button class="new-btn" onclick={handleNew} title="在当前笔记本新建笔记">＋</button>
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
        />
      {/key}
    </main>
  </div>
</div>

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
  }
  .error {
    background: #c0392b;
    color: #fff;
    padding: 6px 14px;
    font-size: 13px;
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
  .new-btn {
    background: none;
    border: none;
    color: var(--accent);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 8px;
    border-radius: 6px;
  }
  .new-btn:hover {
    background: var(--accent-soft);
  }
  .reader {
    overflow-y: auto;
    background: var(--bg);
  }
</style>
