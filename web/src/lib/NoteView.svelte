<script lang="ts">
  import { onDestroy } from 'svelte'
  import { fade } from 'svelte/transition'
  import type { NoteDetail } from './api'
  import { api, taskProgress } from './api'
  import { t } from './i18n.svelte'
  import { renderNote } from './render'
  import Button from './Button.svelte'
  import Editor from './Editor.svelte'
  import WysiwygEditor from './WysiwygEditor.svelte'
  import EditorToolbar from './EditorToolbar.svelte'
  import Icon from './Icon.svelte'
  import type { EditorHandle } from './editor/types'
  import { editorCommands } from './plugins.svelte'

  const ENGINE_KEY = 'jasper.editor'
  // 默认源码模式（无损、所见非所得关闭）；只有用户显式开过富文本才记为 'wysiwyg'。
  function loadEngine(): 'wysiwyg' | 'source' {
    try {
      return localStorage.getItem(ENGINE_KEY) === 'wysiwyg' ? 'wysiwyg' : 'source'
    } catch {
      return 'source'
    }
  }

  let {
    detail,
    onNavigate,
    onChanged,
    onDeleted,
    initialEdit = false,
    readOnly = false,
  }: {
    detail: NoteDetail | null
    onNavigate: (id: string) => void
    onChanged: () => void
    onDeleted: () => void
    initialEdit?: boolean
    readOnly?: boolean
  } = $props()

  // 本组件按笔记 id 在父级以 {#key} 重挂载，故这里用初始值即可，无需响应 detail 变化。
  let editMode = $state(initialEdit)
  let title = $state(detail?.title ?? '')
  let body = $state(detail?.body ?? '')
  // 源码编辑器句柄（就绪后由 Editor 回传），供工具栏命令操作当前源码
  let sourceHandle = $state<EditorHandle | null>(null)

  // 编辑引擎：富文本(Crepe) / 源码(CodeMirror)，记忆在 localStorage。
  // HTML 笔记（markup_language=2）不走 markdown 富文本，强制源码。
  let isMarkdown = $derived(detail?.markup_language !== 2)
  let editorEngine = $state<'wysiwyg' | 'source'>(loadEngine())
  let engine = $derived(isMarkdown ? editorEngine : 'source')
  function toggleEngine() {
    editorEngine = editorEngine === 'wysiwyg' ? 'source' : 'wysiwyg'
    try {
      localStorage.setItem(ENGINE_KEY, editorEngine)
    } catch {
      /* 忽略 */
    }
  }

  let dirty = false
  let saveState = $state<'idle' | 'saving' | 'saved' | 'error'>('idle')
  let timer: ReturnType<typeof setTimeout> | undefined

  // 阅读视图按本地（可能已编辑）的标题/正文渲染，保证切回阅读即时反映改动
  let html = $derived(
    detail ? renderNote({ ...detail, title, body }) : ''
  )
  // 任务清单进度（随正文实时变化）
  let tasks = $derived(taskProgress(body))

  function scheduleSave() {
    if (!detail) return
    dirty = true
    saveState = 'saving'
    clearTimeout(timer)
    timer = setTimeout(save, 800)
  }

  async function save() {
    if (!detail || !dirty) return
    clearTimeout(timer)
    const id = detail.id
    try {
      await api.updateNote(id, { title, body })
      dirty = false
      saveState = 'saved'
      onChanged()
    } catch {
      saveState = 'error'
    }
  }

  function onBodyChange(v: string) {
    body = v
    scheduleSave()
  }

  // 插件 backend 命令（note-toolbar）：把当前正文交给命令，返回的 body 替换编辑缓冲。
  // 仅源码模式暴露（富文本会整篇重排 markdown，替换正文语义不清）。
  let runningCmd = $state<string | null>(null)
  let cmdError = $state('')
  async function runPluginCommand(pluginId: string, commandId: string) {
    if (!detail || runningCmd) return
    runningCmd = commandId
    cmdError = ''
    try {
      const result = await api.runPluginCommand(pluginId, commandId, {
        note_id: detail.id,
        title,
        body,
      })
      if (typeof result.body === 'string' && result.body !== body) {
        body = result.body
        sourceHandle?.setValue(body) // 同步编辑器视图
        scheduleSave() // 走正常自动保存链路
      }
    } catch (e) {
      cmdError = e instanceof Error ? e.message : `${e}`
    } finally {
      runningCmd = null
    }
  }

  async function remove() {
    if (!detail) return
    if (!confirm(t('note.confirmDelete', { title: title || t('common.untitled') }))) return
    try {
      await api.deleteNote(detail.id)
      onDeleted()
    } catch {
      saveState = 'error'
    }
  }

  // 切换笔记/卸载时，若有未保存改动则立即冲刷
  onDestroy(() => {
    if (dirty && detail) {
      clearTimeout(timer)
      const id = detail.id
      api.updateNote(id, { title, body }).then(onChanged).catch(() => {})
    }
  })

  function fmtDateTime(ms: number): string {
    if (!ms) return ''
    return new Date(ms).toLocaleString()
  }

  function onContentClick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest('[data-internal-id]') as HTMLElement | null
    if (!el) return
    e.preventDefault()
    const id = el.getAttribute('data-internal-id')
    if (id) onNavigate(id)
  }
</script>

{#if detail}
  <article class="note-view" in:fade={{ duration: 160 }}>
    <div class="toolbar">
      <div class="left">
        {#if editMode && !readOnly}
          <EditorToolbar mode={engine} handle={sourceHandle} />
          <!-- 插件贡献的编辑器命令（仅源码模式）：一键优化等 -->
          {#if engine === 'source' && editorCommands().length}
            <span class="sep"></span>
            {#each editorCommands() as cmd (cmd.pluginId + ':' + cmd.commandId)}
              <Button
                variant="ghost"
                iconOnly
                icon={runningCmd === cmd.commandId ? 'clean' : cmd.icon}
                label={cmd.title}
                onclick={() => runPluginCommand(cmd.pluginId, cmd.commandId)}
                disabled={!sourceHandle || runningCmd !== null}
              />
            {/each}
          {/if}
        {/if}
        {#if editMode}
          <span class="save-state {saveState}">
            {#if cmdError}
              <span class="cmd-error" title={cmdError}><Icon name="alert" size={12} /> {t('plugins.cmdFailed')}</span>
            {:else}
              {saveState === 'saving' ? t('note.saving') : saveState === 'saved' ? t('note.saved') : saveState === 'error' ? t('note.saveFailed') : ''}
            {/if}
          </span>
        {/if}
      </div>
      <div class="right">
        {#if !readOnly}
          {#if editMode && isMarkdown}
            <Button
              variant="default"
              icon={engine === 'wysiwyg' ? 'code' : 'rich'}
              label={engine === 'wysiwyg' ? t('note.toSource') : t('note.toRich')}
              onclick={toggleEngine}
            />
          {/if}
          <Button
            variant="default"
            icon={editMode ? 'eye' : 'edit'}
            label={editMode ? t('note.read') : t('note.edit')}
            onclick={() => (editMode = !editMode)}
          />
          <Button variant="danger" icon="trash" label={t('note.delete')} onclick={remove} />
        {/if}
      </div>
    </div>

    {#if editMode}
      <input
        class="title-input"
        bind:value={title}
        oninput={scheduleSave}
        placeholder={t('note.titlePlaceholder')}
      />
      <div class="editor-wrap">
        {#if engine === 'wysiwyg'}
          <WysiwygEditor value={body} onChange={onBodyChange} />
        {:else}
          <Editor value={body} onChange={onBodyChange} onReady={(h) => (sourceHandle = h)} />
        {/if}
      </div>
    {:else}
      <h1 class="note-title">{title || t('common.untitled')}</h1>
      <div class="meta">
        {t('note.updatedAt', { time: fmtDateTime(detail.updated_time) })}
        {#if detail.source_url}
          · <a href={detail.source_url} target="_blank" rel="noopener noreferrer">{t('note.source')}</a>
        {/if}
        {#if detail.markup_language === 2}<span class="badge">HTML</span>{/if}
        {#if tasks[1] > 0}
          <span class="task-meta" class:done={tasks[0] === tasks[1]}>
            <span class="bar"><span class="fill" style="width:{Math.round((tasks[0] / tasks[1]) * 100)}%"></span></span>
            {t('list.tasks', { done: tasks[0], total: tasks[1] })}
          </span>
        {/if}
      </div>
      <!-- 滚动容器占满整栏宽 → 滚动条贴窗口右缘；内层 .content 才是阅读宽度 -->
      <div class="content-scroll">
        <!-- 内容已由 DOMPurify 净化 -->
        <div class="content" onclick={onContentClick}>{@html html}</div>
      </div>
    {/if}
  </article>
{:else}
  <div class="placeholder">{t('note.placeholder')}</div>
{/if}

<style>
  .note-view {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    /* 高度/下边框与左两栏 .pane-title(38px) 对齐，三栏顶栏齐平 */
    height: 38px;
    box-sizing: border-box;
    padding: 0 8px 0 12px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .save-state {
    font-size: 12px;
    color: var(--text-dim);
    transition: color 0.2s ease;
  }
  .save-state.saved {
    color: var(--success);
  }
  .save-state.error {
    color: var(--danger);
  }
  .cmd-error {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--danger);
  }
  .sep {
    flex: 0 0 auto;
    width: 1px;
    height: 18px;
    background: var(--border);
    margin: 0 3px;
  }
  .left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
  }
  .right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
  }
  .title-input {
    border: none;
    outline: none;
    background: none;
    color: var(--text);
    font-size: 22px;
    font-weight: 700;
    padding: 18px 36px 8px;
    flex: 0 0 auto;
  }
  .editor-wrap {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 0 20px 20px;
  }
  .note-title {
    font-size: 24px;
    margin: 0;
    padding: 24px 36px 6px;
  }
  .meta {
    font-size: 12px;
    color: var(--text-dim);
    padding: 0 36px 14px;
    border-bottom: 1px solid var(--border);
  }
  .badge {
    background: var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    margin-left: 6px;
  }
  .task-meta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 8px;
    font-variant-numeric: tabular-nums;
  }
  .task-meta .bar {
    width: 60px;
    height: 5px;
    border-radius: 3px;
    background: var(--hover);
    overflow: hidden;
  }
  .task-meta .fill {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .task-meta.done {
    color: var(--success);
  }
  .task-meta.done .fill {
    background: var(--success);
  }
  /* 全宽滚动容器：滚动条落在整栏（即窗口）右缘 */
  .content-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  /* 阅读宽度盒：内容左对齐限宽，滚动条不受其 max-width 约束 */
  .content {
    padding: 20px 36px 80px;
    max-width: 820px;
  }
  .placeholder {
    display: flex;
    height: 100%;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
  }
</style>
