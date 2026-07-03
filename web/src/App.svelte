<script lang="ts">
  import { onMount } from 'svelte'
  import { slide } from 'svelte/transition'
  import {
    api,
    IS_DEMO,
    FOLDER_DND_TYPE,
    setAuthErrorHandler,
    type FolderNode,
    type NoteSummary,
    type NoteDetail,
    type PendingWrite,
    type StatusResp,
  } from './lib/api'
  import AuthDialog from './lib/AuthDialog.svelte'
  import { draggingFolder } from './lib/dnd.svelte'
  import { t, getLocale, toggleLocale } from './lib/i18n.svelte'
  import Button from './lib/Button.svelte'
  import Icon from './lib/Icon.svelte'
  import ThemePicker from './lib/ThemePicker.svelte'
  import FolderTree from './lib/FolderTree.svelte'
  import NoteList from './lib/NoteList.svelte'
  import NoteView from './lib/NoteView.svelte'
  import Settings from './lib/Settings.svelte'
  import ResourcePanel from './lib/ResourcePanel.svelte'
  import PluginPanel from './lib/PluginPanel.svelte'
  import PluginSidebar from './lib/PluginSidebar.svelte'
  import PendingWriteDialog from './lib/PendingWriteDialog.svelte'
  import { loadPlugins, pluginsAvailable, sidebarContributions, type SidebarEntry } from './lib/plugins.svelte'
  import { connectEvents, type ChangeEvent } from './lib/events'

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
  // NoteView 组件实例（bind:this）：SSE 外部变更经 applyExternal 保守回显
  let noteView = $state<ReturnType<typeof NoteView> | null>(null)

  let query = $state('')
  let searchMode = $state(false)
  let listTitle = $state('')
  let error = $state('')

  let configured = $state<boolean | null>(null)
  let showSettings = $state(false)
  let showResources = $state(false)
  let showPlugins = $state(false)
  let showDemoBanner = $state(true)

  // 访问鉴权（access control）状态（来自 /api/status）
  let authEnabled = $state(false) // 是否设了访问密码
  let authenticated = $state(false) // 本会话是否已登录
  let passwordlessRead = $state(false) // 允许无密码阅读
  let showAuthDialog = $state(false) // 登录框开关
  // 受保护但未登录：写入闸门收紧为只读（下方 readOnly 合并）；且当无可见内容时给登录提示
  const locked = $derived(authEnabled && !authenticated)

  // 插件侧边栏（右侧 dock，spec §3.5/§9.4）：当前打开的面板；插件被禁用/卸载后自动关闭
  let dockEntry = $state<SidebarEntry | null>(null)
  $effect(() => {
    if (
      dockEntry &&
      !sidebarContributions().some(
        (e) => e.pluginId === dockEntry!.pluginId && e.contribution.id === dockEntry!.contribution.id,
      )
    ) {
      dockEntry = null
    }
  })
  function toggleDock(entry: SidebarEntry) {
    dockEntry =
      dockEntry?.pluginId === entry.pluginId && dockEntry.contribution.id === entry.contribution.id
        ? null
        : entry
  }

  // 插件写提案获批落盘后：受影响的打开笔记强制重挂载取新内容，列表/树跟着刷新
  async function onPluginWriteApplied(w: PendingWrite) {
    try {
      if (w.action === 'update' && selectedNoteId === w.note.id) {
        const id = w.note.id
        selectedNoteId = null
        detail = null
        await selectNote(id)
      }
      if (w.action === 'create') folders = await api.folders()
      await refreshList()
    } catch {
      /* 忽略 */
    }
  }
  // 服务端只读模式（/api/status 返回）。与编译期 demo 只读、未登录锁定合并成统一的写入闸门。
  let serverReadOnly = $state(false)
  const readOnly = $derived(IS_DEMO || serverReadOnly || locked)

  // 把一次 /api/status 的开关同步进本地状态（checkStatus / onConfigured / 登录后共用）。
  function applyStatusFlags(s: StatusResp) {
    serverReadOnly = s.read_only
    authEnabled = s.auth_enabled
    authenticated = s.authenticated
    passwordlessRead = s.passwordless_read
  }

  // 登录成功：关闭登录框并整体重载（现在可写 + 可见全部）
  async function onLoginSuccess() {
    showAuthDialog = false
    await checkStatus()
  }
  // 登出：吊销会话 + 清 token → 重载（回到匿名可见范围）
  async function doLogout() {
    try {
      await api.logout()
    } catch {
      /* 忽略 */
    }
    await checkStatus()
  }

  // 访问控制设置在设置页被改动后：刷新状态闸门（不重载数据、不关设置页）
  async function refreshStatus() {
    try {
      applyStatusFlags(await api.status())
    } catch {
      /* 忽略 */
    }
  }

  // ---- SSE 变更 → 去抖合并刷新（/api/events）----
  // 自己的写入也会回声：列表刷新幂等、打开中的笔记走 NoteView.applyExternal 的
  // §5.3 保守规则（内容相同/正在输入都不动缓冲），所以无需区分事件来源。
  const remotePending = { folders: false, list: false, openNote: false, openNoteDeleted: false }
  let remoteTimer: ReturnType<typeof setTimeout> | undefined

  function onRemoteChange(ev: ChangeEvent) {
    if (ev.kind === 'library') {
      // 整库替换（数据源切换/服务重启后重连）：全量重载
      clearTimeout(remoteTimer)
      remotePending.folders = remotePending.list = remotePending.openNote = false
      remotePending.openNoteDeleted = false
      void checkStatus()
      return
    }
    if (ev.kind === 'folder') {
      remotePending.folders = remotePending.list = true
    } else {
      remotePending.list = true
      // 未知 id 的 upsert 可能是新建/移动（笔记本计数变了）；删除同理
      const known = notes.some((n) => n.id === ev.id)
      if (ev.op === 'delete' || !known) remotePending.folders = true
      if (ev.id === selectedNoteId) {
        if (ev.op === 'delete') remotePending.openNoteDeleted = true
        else remotePending.openNote = true
      }
    }
    clearTimeout(remoteTimer)
    remoteTimer = setTimeout(applyRemoteChanges, 250)
  }

  async function applyRemoteChanges() {
    const { folders: doFolders, list: doList, openNote, openNoteDeleted } = remotePending
    remotePending.folders = remotePending.list = remotePending.openNote = false
    remotePending.openNoteDeleted = false
    try {
      if (openNoteDeleted) {
        // 打开中的笔记被外部删除：关视图（未保存输入已无处可保存）
        selectedNoteId = null
        detail = null
        saveLastNoteId(null)
      }
      if (doFolders) folders = await api.folders()
      if (doList) await refreshList()
      if (openNote && !openNoteDeleted && selectedNoteId) {
        const fresh = await api.note(selectedNoteId)
        if (noteView?.applyExternal(fresh)) detail = fresh
      }
    } catch {
      /* 网络抖动忽略；下一个事件会再触发 */
    }
  }

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

  onMount(() => {
    // token 失效（服务端重启/改密码使会话作废）→ 回退只读并重新拉状态
    setAuthErrorHandler(() => {
      authenticated = false
      void checkStatus()
    })
    void checkStatus()
  })

  async function checkStatus() {
    // 插件列表并行加载（探测服务端是否编译 plugins feature；注入插件主题 CSS）
    void loadPlugins()
    try {
      const s = await api.status()
      configured = s.configured
      applyStatusFlags(s)
      if (configured) {
        connectEvents(onRemoteChange) // 幂等；DEMO/不支持时静默跳过
        await loadFolders()
      }
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
        // 不传显式 title：合成的「未分类笔记」节点(id=="")后端留空标题，交给 findTitle 按语言取词
        selectFolder(first.id)
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
    // 只读/鉴权开关可能在设置里被切换 → 重新拉状态刷新写入闸门
    try {
      applyStatusFlags(await api.status())
    } catch {
      /* 忽略 */
    }
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
    // 合成的「未分类笔记」节点（id==""）标题由后端留空，这里按当前语言取词。
    if (id === '') return t('tree.unfiled')
    for (const f of list) {
      if (f.id === id) return f.title
      const child = findTitle(f.children, id)
      if (child) return child
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

  // 拖拽：把笔记移动到另一个笔记本（改 parent_id）
  async function moveNote(noteId: string, targetFolderId: string) {
    try {
      const d = await api.moveNote(noteId, targetFolderId)
      if (selectedNoteId === noteId) detail = d // 当前打开的就是它 → 同步详情
      folders = await api.folders() // 源/目标笔记本篇数变化
      await refreshList() // 移走的笔记从当前列表消失（剩余项 FLIP 平滑补位）
    } catch (e) {
      error = `${e}`
    }
  }

  // 拖拽：把笔记本移到另一个笔记本下（parentId 空=移到顶层）。后端防环。
  async function moveFolder(folderId: string, parentId: string) {
    try {
      await api.moveFolder(folderId, parentId)
      folders = await api.folders()
    } catch (e) {
      error = `${e}`
    }
  }

  // 重命名笔记本：名称走浏览器 prompt（预填当前名）。改完刷新树；若正是当前所选，同步列表标题。
  async function renameFolder(folderId: string, currentTitle: string) {
    const name = (prompt(t('notebook.renamePrompt'), currentTitle) ?? '').trim()
    if (!name || name === currentTitle) return
    try {
      await api.renameFolder(folderId, name)
      folders = await api.folders()
      if (!searchMode && selectedFolderId === folderId) listTitle = name
    } catch (e) {
      error = `${e}`
    }
  }

  // 新建笔记本（顶层）：名称走浏览器 prompt（带本地化默认名），建好后选中
  async function handleNewFolder() {
    const name = (prompt(t('notebook.namePrompt'), t('notebook.defaultName')) ?? '').trim()
    if (!name) return
    try {
      const f = await api.createFolder({ parent_id: '', title: name })
      folders = await api.folders()
      selectFolder(f.id, f.title)
    } catch (e) {
      error = `${e}`
    }
  }

  // 新建待办：与新建笔记同路径，仅 is_todo=true
  async function handleNewTodo() {
    const parent = searchMode ? '' : selectedFolderId ?? ''
    try {
      const n = await api.createNote({ parent_id: parent, title: t('note.newTodoTitle'), body: '', is_todo: true })
      editOnOpenId = n.id
      detail = n
      selectedNoteId = n.id
      saveLastNoteId(n.id)
      // 之前未选中任何笔记本（如空白库）时，把选中态落到笔记实际所在的 parent，
      // 否则 refreshList 的 selectedFolderId!=null 判断会跳过拉取，新笔记不会出现在列表里。
      if (!searchMode && selectedFolderId == null) {
        selectedFolderId = parent // parent 在此分支恒为 ''（未分类笔记）
        listTitle = t('tree.unfiled')
      }
      folders = await api.folders() // 未分类笔记数/笔记本篇数变化
      await refreshList()
    } catch (e) {
      error = `${e}`
    }
  }

  // 「移到顶层」根放置区（仅拖拽笔记本时显示）
  let rootDropOver = $state(false)
  function onRootDragOver(e: DragEvent) {
    if (!e.dataTransfer?.types.includes(FOLDER_DND_TYPE)) return
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    rootDropOver = true
  }
  function onRootDrop(e: DragEvent) {
    rootDropOver = false
    const id = e.dataTransfer?.getData(FOLDER_DND_TYPE)
    if (id) moveFolder(id, '')
  }

  async function handleNew() {
    const parent = searchMode ? '' : selectedFolderId ?? ''
    try {
      const n = await api.createNote({ parent_id: parent, title: t('note.newNote'), body: '' })
      editOnOpenId = n.id
      detail = n
      selectedNoteId = n.id
      saveLastNoteId(n.id)
      // 之前未选中任何笔记本（如空白库）时，把选中态落到笔记实际所在的 parent，
      // 否则 refreshList 的 selectedFolderId!=null 判断会跳过拉取，新笔记不会出现在列表里。
      if (!searchMode && selectedFolderId == null) {
        selectedFolderId = parent // parent 在此分支恒为 ''（未分类笔记）
        listTitle = t('tree.unfiled')
      }
      folders = await api.folders() // 未分类笔记数/笔记本篇数变化
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
    {#if readOnly}
      <span class="ro-badge" title={t('common.readOnlyTitle')}>{t('common.readOnly')}</span>
    {/if}
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
        {#if locked}
          <!-- 受保护未登录：只给「解锁登录」入口，管理功能待登录后出现 -->
          <Button variant="ghost" iconOnly icon="lock" label={t('auth.unlock')} onclick={() => (showAuthDialog = true)} />
        {:else}
          {#if authenticated}
            <Button variant="ghost" iconOnly icon="unlock" label={t('auth.lock')} onclick={doLogout} />
          {/if}
          <Button variant="ghost" iconOnly icon="image" label={t('topbar.resources')} onclick={() => (showResources = true)} />
          {#if pluginsAvailable()}
            <Button variant="ghost" iconOnly icon="plug" label={t('plugins.topbar')} onclick={() => (showPlugins = true)} />
          {/if}
          <Button variant="ghost" iconOnly icon="settings" label={t('topbar.settings')} onclick={() => (showSettings = true)} />
        {/if}
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

  {#if locked && configured && folders.length === 0}
    <!-- 受保护、未登录、且无任何可见内容（整站私有）→ 登录闸门取代三栏 -->
    <div class="locked-gate">
      <Icon name="lock" size={40} />
      <h2>{t('auth.lockedTitle')}</h2>
      <p>{t('auth.lockedDesc')}</p>
      <Button variant="primary" icon="lock" label={t('auth.unlock')} onclick={() => (showAuthDialog = true)} />
    </div>
  {:else}
    <div class="panes" class:with-dock={dockEntry != null}>
    <aside class="sidebar">
      <div class="pane-title">
        <span>{t('pane.notebooks')}</span>
        {#if !readOnly}
          <Button variant="ghost" iconOnly icon="folder-plus" label={t('pane.newNotebook')} onclick={handleNewFolder} />
        {/if}
      </div>
      {#if !readOnly && draggingFolder()}
        <div
          class="root-drop"
          class:over={rootDropOver}
          role="presentation"
          transition:slide={{ duration: 150 }}
          ondragover={onRootDragOver}
          ondragleave={() => (rootDropOver = false)}
          ondrop={onRootDrop}
        >
          {t('tree.moveToTop')}
        </div>
      {/if}
      <FolderTree
        {folders}
        selectedId={searchMode ? null : selectedFolderId}
        onSelect={(id) => selectFolder(id)}
        onMoveNote={readOnly ? undefined : moveNote}
        onMoveFolder={readOnly ? undefined : moveFolder}
        onRenameFolder={readOnly ? undefined : renameFolder}
      />
      <!-- 插件面板入口（左栏底部，spec §9.4）；只读下命令端点全被拦，一并隐藏 -->
      {#if !readOnly && sidebarContributions().length}
        <div class="plugin-entries">
          {#each sidebarContributions() as entry (entry.pluginId + '/' + entry.contribution.id)}
            <button
              class="plugin-entry"
              class:active={dockEntry?.pluginId === entry.pluginId &&
                dockEntry.contribution.id === entry.contribution.id}
              title={entry.pluginName}
              onclick={() => toggleDock(entry)}
            >
              <Icon name={entry.contribution.icon || 'plug'} size={14} />
              <span class="entry-title">{entry.contribution.title}</span>
            </button>
          {/each}
        </div>
      {/if}
    </aside>

    <section class="notelist">
      <div class="pane-title">
        <span>{listTitle}</span>
        {#if !readOnly}
          <span class="title-actions">
            <Button variant="ghost" iconOnly icon="check-square" label={t('pane.newTodo')} onclick={handleNewTodo} />
            <Button variant="ghost" iconOnly icon="plus" label={t('pane.newNote')} onclick={handleNew} />
          </span>
        {/if}
      </div>
      <NoteList {notes} selectedId={selectedNoteId} onSelect={selectNote} canDrag={!readOnly} />
    </section>

    <main class="reader">
      {#key selectedNoteId}
        <NoteView
          bind:this={noteView}
          {detail}
          onNavigate={navigate}
          onChanged={refreshList}
          onDeleted={onNoteDeleted}
          initialEdit={detail != null && detail.id === editOnOpenId}
          {readOnly}
        />
      {/key}
    </main>

    {#if dockEntry}
      <aside class="dock">
        {#key dockEntry.pluginId + '/' + dockEntry.contribution.id}
          <PluginSidebar
            entry={dockEntry}
            noteId={selectedNoteId}
            onClose={() => (dockEntry = null)}
            onNotesChanged={refreshList}
          />
        {/key}
      </aside>
    {/if}
    </div>
  {/if}
</div>

<PendingWriteDialog onApplied={onPluginWriteApplied} />

{#if showAuthDialog}
  <AuthDialog onSuccess={onLoginSuccess} onClose={() => (showAuthDialog = false)} />
{/if}

{#if configured === false}
  <Settings mode="setup" onDone={onConfigured} />
{:else if showSettings}
  <Settings
    mode="settings"
    onDone={onConfigured}
    onClose={() => (showSettings = false)}
    onAuthChanged={refreshStatus}
  />
{/if}

{#if showPlugins}
  <PluginPanel {readOnly} onClose={() => (showPlugins = false)} />
{/if}

{#if showResources}
  <ResourcePanel {readOnly} onClose={() => (showResources = false)} onChanged={onResourcesChanged} />
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
  .ro-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-side);
    color: var(--text-dim);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    flex: 0 0 auto;
    cursor: default;
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
  /* 登录闸门（整站私有、未登录）：占据三栏位置的居中提示 */
  .locked-gate {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px 20px;
    color: var(--text-dim);
    text-align: center;
  }
  .locked-gate h2 {
    margin: 0;
    font-size: 18px;
    color: var(--text);
  }
  .locked-gate p {
    margin: 0;
    font-size: 14px;
    max-width: 360px;
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
  /* 插件面板 dock：在阅读区右侧挤出第四栏（spec §9.4，约 320px） */
  .panes.with-dock {
    grid-template-columns: 250px 300px 1fr 320px;
  }
  .dock {
    border-left: 1px solid var(--border);
    overflow: hidden;
    min-height: 0;
    background: var(--bg-side);
  }
  .sidebar {
    border-right: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0 0 12px;
    background: var(--bg-side);
    display: flex;
    flex-direction: column;
  }
  .sidebar :global(> *) {
    flex: 0 0 auto;
  }
  /* 插件入口钉在左栏底部 */
  .plugin-entries {
    margin-top: auto;
    padding: 8px 8px 0;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .plugin-entry {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 8px;
    border: none;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    text-align: left;
    border-radius: 7px;
    cursor: pointer;
  }
  .plugin-entry:hover {
    background: var(--hover);
  }
  .plugin-entry.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .entry-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .title-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .root-drop {
    margin: 6px 8px;
    padding: 9px;
    border: 1px dashed var(--border);
    border-radius: 8px;
    text-align: center;
    font-size: 12px;
    color: var(--text-dim);
    transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
  }
  .root-drop.over {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
</style>
