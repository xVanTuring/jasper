<script lang="ts">
  import { onMount } from 'svelte'
  import { fade, scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { api, type AiConfig, type AuthListMode, type FolderNode } from './api'
  import { t } from './i18n.svelte'
  import Button from './Button.svelte'
  import Icon from './Icon.svelte'
  import SchemaForm from './SchemaForm.svelte'
  import { defaultValues, validate, type FieldError } from './schema'
  import { loadPlugins, pluginsAvailable, pluginsLoaded, storageProviders, type StorageProvider } from './plugins.svelte'

  let {
    mode,
    onDone,
    onClose,
    onAuthChanged,
  }: {
    mode: 'setup' | 'settings'
    onDone: () => void
    onClose?: () => void
    // 访问控制设置变更后通知父组件刷新 /api/status（更新只读闸门/登录态）
    onAuthChanged?: () => void
  } = $props()

  let libMode = $state<'existing' | 'new'>('existing')
  // 'local' | 'webdav' | 插件 provider 键 'plugin:<插件id>:<贡献id>'
  let sourceType = $state<string>('local')
  let localPath = $state('')
  let webdavUrl = $state('')
  let webdavUser = $state('')
  let webdavPass = $state('')
  let readOnly = $state(false)
  // 插件数据源的表单值与校验错误（SchemaForm 驱动）
  let pluginConfig = $state<Record<string, unknown>>({})
  let formErrors = $state<Partial<Record<string, FieldError>>>({})
  // 设置模式回显的 provider 键与配置（切换 provider 时用于恢复）
  let prefillKey = $state('')
  let prefillConfig: Record<string, unknown> = {}

  let submitting = $state(false)
  let error = $state('')

  const providerKeyOf = (p: StorageProvider) => `plugin:${p.pluginId}:${p.contribution.id}`
  const currentProvider = $derived(storageProviders().find((p) => providerKeyOf(p) === sourceType))
  // 回显的 provider 已消失（插件被卸载/停用）→ 警告并禁提交
  const prefillMissing = $derived(
    prefillKey !== '' && pluginsLoaded() && !storageProviders().some((p) => providerKeyOf(p) === prefillKey),
  )

  function selectProvider(p: StorageProvider) {
    const key = providerKeyOf(p)
    if (sourceType === key) return
    sourceType = key
    formErrors = {}
    pluginConfig =
      key === prefillKey ? { ...prefillConfig } : defaultValues(p.contribution.config_schema)
  }

  // provider 图标：manifest 声明的令牌存在则用之，否则回落 plug
  function providerIcon(p: StorageProvider): string {
    const name = p.contribution.icon
    if (!name) return 'plug'
    const v = getComputedStyle(document.documentElement).getPropertyValue(`--icon-${name}`)
    return v.trim() ? name : 'plug'
  }

  onMount(async () => {
    loadPlugins() // 幂等；保证 providers 可选（含设置页直开的场景）
    // 设置模式下预填当前配置
    if (mode === 'settings') {
      try {
        const c = await api.getConfig()
        localPath = c.local_path
        webdavUrl = c.webdav_url
        webdavUser = c.webdav_user
        webdavPass = c.webdav_pass
        readOnly = c.read_only
        if (c.source_type === 'webdav') sourceType = 'webdav'
        else if (c.source_type === 'plugin') {
          // 与 webdav_pass 同姿势：plugin_config（含 secret）由 GET /api/config 回显以支持预填
          prefillKey = `plugin:${c.plugin_id}:${c.plugin_storage}`
          try {
            prefillConfig = JSON.parse(c.plugin_config || '{}')
          } catch {
            prefillConfig = {}
          }
          sourceType = prefillKey
          pluginConfig = { ...prefillConfig }
        } else if (c.source_type === 'local') sourceType = 'local'
      } catch {
        /* 忽略 */
      }
    }
  })

  // ---------- 宿主级 AI 配置（host:ai，spec 0.3 §9.5）----------
  // 独立于数据源表单，单独保存；仅设置模式且服务端带 plugins feature 时出现。
  let aiCfg = $state<AiConfig>({ provider: '', base_url: '', api_key: '', model: '' })
  let aiLoaded = $state(false)
  let aiSaving = $state(false)
  let aiSaved = $state(false)
  let aiError = $state('')

  $effect(() => {
    if (mode === 'settings' && pluginsAvailable() && !aiLoaded) {
      aiLoaded = true
      void (async () => {
        try {
          aiCfg = await api.getAiConfig()
        } catch {
          aiError = t('settings.ai.loadFailed')
        }
      })()
    }
  })

  async function saveAi() {
    aiSaving = true
    aiError = ''
    aiSaved = false
    try {
      await api.saveAiConfig(aiCfg)
      aiSaved = true
      setTimeout(() => (aiSaved = false), 2000)
    } catch (e) {
      aiError = e instanceof Error ? e.message : `${e}`
    } finally {
      aiSaving = false
    }
  }

  // ---------- 访问控制（access control）----------
  // 仅设置模式出现。密码只回 password_set 布尔；名单从笔记本树展平供勾选。
  let authLoaded = $state(false)
  let authPasswordSet = $state(false)
  let authNewPassword = $state('') // 新密码输入（留空则不改）
  let authPasswordless = $state(false)
  let authListMode = $state<AuthListMode>('none')
  let authFolderList = $state<string[]>([])
  let authFolders = $state<{ id: string; title: string; depth: number }[]>([])
  let authSaving = $state(false)
  let authSaved = $state(false)
  let authError = $state('')

  $effect(() => {
    if (mode === 'settings' && !authLoaded) {
      authLoaded = true
      void loadAuth()
    }
  })

  async function loadAuth() {
    try {
      const s = await api.getAuthSettings()
      authPasswordSet = s.password_set
      authPasswordless = s.passwordless_read
      authListMode = s.list_mode
      authFolderList = s.folder_list
    } catch {
      /* 无权/未登录 → 保持默认（首次设密码前人人可读） */
    }
    try {
      authFolders = flattenFolders(await api.folders())
    } catch {
      authFolders = []
    }
  }

  // 展平笔记本树（跳过合成的未分类节点 id=""），带缩进层级供勾选显示。
  function flattenFolders(nodes: FolderNode[], depth = 0): { id: string; title: string; depth: number }[] {
    const out: { id: string; title: string; depth: number }[] = []
    for (const n of nodes) {
      if (n.id) out.push({ id: n.id, title: n.title, depth })
      out.push(...flattenFolders(n.children, depth + 1))
    }
    return out
  }

  function toggleFolderInList(id: string) {
    authFolderList = authFolderList.includes(id)
      ? authFolderList.filter((x) => x !== id)
      : [...authFolderList, id]
  }

  async function saveAuth(clearPassword = false) {
    authSaving = true
    authError = ''
    authSaved = false
    const newPassword = clearPassword ? '' : authNewPassword.trim()
    try {
      const s = await api.saveAuthSettings({
        password: newPassword || undefined,
        clear_password: clearPassword,
        passwordless_read: authPasswordless,
        list_mode: authListMode,
        folder_list: authFolderList,
      })
      authPasswordSet = s.password_set
      authPasswordless = s.passwordless_read
      authListMode = s.list_mode
      authFolderList = s.folder_list
      // 设/改密码会吊销所有会话（含本人）→ 用刚设的密码自动重登，保持当前管理员在线。
      if (newPassword) {
        try {
          await api.login(newPassword)
        } catch {
          /* 忽略：父组件刷新状态后会显示登录闸门 */
        }
      }
      authNewPassword = ''
      authSaved = true
      setTimeout(() => (authSaved = false), 2000)
      onAuthChanged?.() // 通知父组件刷新只读闸门/登录态
    } catch (e) {
      authError = e instanceof Error ? e.message : `${e}`
    } finally {
      authSaving = false
    }
  }

  async function submit() {
    error = ''
    formErrors = {}
    let pluginPayload: { plugin_id: string; plugin_storage: string; plugin_config: Record<string, unknown> } = {
      plugin_id: '',
      plugin_storage: '',
      plugin_config: {},
    }
    let effectiveType = sourceType
    if (sourceType.startsWith('plugin:')) {
      const p = currentProvider
      if (!p) {
        error = t('settings.providerMissing')
        return
      }
      const v = validate(p.contribution.config_schema, pluginConfig)
      formErrors = v.errors
      if (!v.ok) return
      effectiveType = 'plugin'
      pluginPayload = {
        plugin_id: p.pluginId,
        plugin_storage: p.contribution.id,
        plugin_config: v.cleaned,
      }
    }
    submitting = true
    try {
      const res = await api.saveConfig({
        source_type: effectiveType,
        local_path: localPath.trim(),
        webdav_url: webdavUrl.trim(),
        webdav_user: webdavUser,
        webdav_pass: webdavPass,
        ...pluginPayload,
        read_only: readOnly,
        create_new: libMode === 'new',
      })
      if (res.ok) {
        onDone()
      } else {
        error = res.error || t('settings.connectFailed')
      }
    } catch (e) {
      error = `${e}`
    } finally {
      submitting = false
    }
  }
</script>

<div class="overlay" class:setup={mode === 'setup'} transition:fade={{ duration: 150 }}>
  <div class="card" transition:scale={{ duration: 190, start: 0.96, opacity: 0, easing: cubicOut }}>
    <header>
      <h2>{mode === 'setup' ? t('settings.welcomeTitle') : t('settings.title')}</h2>
      {#if mode === 'settings'}
        <Button variant="ghost" iconOnly icon="close" label={t('common.close')} onclick={() => onClose?.()} />
      {/if}
    </header>

    {#if mode === 'setup'}
      <p class="hint">{t('settings.firstHint')}</p>
    {/if}

    <!-- 现有 / 新建 -->
    <div class="seg">
      <button class:on={libMode === 'existing'} onclick={() => (libMode = 'existing')}>
        {t('settings.useExisting')}
      </button>
      <button class:on={libMode === 'new'} onclick={() => (libMode = 'new')}>
        {t('settings.createNew')}
      </button>
    </div>
    <p class="sub">
      {libMode === 'existing' ? t('settings.existingDesc') : t('settings.newDesc')}
    </p>

    <!-- 本地 / WebDAV / 插件 provider（动态） -->
    <div class="seg seg-wrap">
      <button class:on={sourceType === 'local'} onclick={() => (sourceType = 'local')}>
        <Icon name="folder" size={14} /> {t('settings.local')}
      </button>
      <button class:on={sourceType === 'webdav'} onclick={() => (sourceType = 'webdav')}>
        <Icon name="cloud" size={14} /> {t('settings.webdav')}
      </button>
      {#each storageProviders() as p (providerKeyOf(p))}
        <button class:on={sourceType === providerKeyOf(p)} onclick={() => selectProvider(p)}>
          <Icon name={providerIcon(p)} size={14} /> {p.contribution.name}
        </button>
      {/each}
    </div>

    {#if prefillMissing && sourceType === prefillKey}
      <div class="error"><Icon name="alert" size={14} /> {t('settings.providerMissing')}</div>
    {/if}

    {#if sourceType === 'local'}
      <label>
        {t('settings.folderPath')}
        <input
          type="text"
          bind:value={localPath}
          placeholder={libMode === 'new' ? t('settings.localPhNew') : t('settings.localPhExisting')}
        />
      </label>
      <p class="tip">{t('settings.localTip')}</p>
    {:else if sourceType === 'webdav'}
      <label>
        {t('settings.webdavUrl')}
        <input type="text" bind:value={webdavUrl} placeholder={t('settings.webdavUrlPh')} />
      </label>
      <label>
        {t('settings.username')}
        <input type="text" bind:value={webdavUser} autocomplete="username" />
      </label>
      <label>
        {t('settings.password')}
        <input type="password" bind:value={webdavPass} autocomplete="current-password" />
      </label>
    {:else if currentProvider}
      <div class="plugin-form">
        <SchemaForm
          schema={currentProvider.contribution.config_schema}
          bind:values={pluginConfig}
          errors={formErrors}
        />
      </div>
    {/if}

    <label class="toggle">
      <input type="checkbox" bind:checked={readOnly} />
      <span class="toggle-body">
        <span class="toggle-title">{t('settings.readOnly')}</span>
        <span class="toggle-desc">{t('settings.readOnlyDesc')}</span>
      </span>
    </label>

    {#if mode === 'settings' && pluginsAvailable()}
      <div class="ai-section">
        <h3>{t('settings.ai.title')}</h3>
        <p class="hint">{t('settings.ai.desc')}</p>
        <label class="ai-field">
          <span>{t('settings.ai.provider')}</span>
          <select bind:value={aiCfg.provider}>
            <option value="">{t('settings.ai.providerNone')}</option>
            <option value="anthropic">Anthropic</option>
            <option value="openai">OpenAI API</option>
          </select>
        </label>
        {#if aiCfg.provider}
          <label class="ai-field">
            <span>{t('settings.ai.baseUrl')}</span>
            <input type="text" bind:value={aiCfg.base_url} placeholder={t('settings.ai.baseUrlPh')} />
          </label>
          <label class="ai-field">
            <span>{t('settings.ai.apiKey')}</span>
            <input type="password" bind:value={aiCfg.api_key} autocomplete="off" />
          </label>
          <label class="ai-field">
            <span>{t('settings.ai.model')}</span>
            <input type="text" bind:value={aiCfg.model} placeholder={t('settings.ai.modelPh')} />
          </label>
        {/if}
        {#if aiError}
          <div class="error"><Icon name="alert" size={14} /> {aiError}</div>
        {/if}
        <div class="ai-actions">
          {#if aiSaved}<span class="saved">{t('settings.ai.saved')}</span>{/if}
          <Button label={t('settings.ai.save')} onclick={saveAi} disabled={aiSaving} />
        </div>
      </div>
    {/if}

    <!-- 访问控制（access control）：设置模式独立段 -->
    {#if mode === 'settings'}
      <div class="ai-section">
        <h3>{t('settings.auth.title')}</h3>
        <p class="hint">{t('settings.auth.desc')}</p>
        <label class="ai-field">
          <span>{t('settings.auth.password')}</span>
          <input
            type="password"
            bind:value={authNewPassword}
            autocomplete="new-password"
            placeholder={authPasswordSet
              ? t('settings.auth.passwordChangePh')
              : t('settings.auth.passwordSetPh')}
          />
        </label>
        {#if authPasswordSet}
          <p class="tip">{t('settings.auth.passwordSet')}</p>
        {/if}

        <label class="toggle">
          <input type="checkbox" bind:checked={authPasswordless} />
          <span class="toggle-body">
            <span class="toggle-title">{t('settings.auth.passwordlessRead')}</span>
            <span class="toggle-desc">{t('settings.auth.passwordlessReadDesc')}</span>
          </span>
        </label>

        {#if authPasswordless}
          <div class="auth-scope">
            <span class="scope-label">{t('settings.auth.listMode')}</span>
            <div class="seg">
              <button class:on={authListMode === 'none'} onclick={() => (authListMode = 'none')}>
                {t('settings.auth.listModeNone')}
              </button>
              <button class:on={authListMode === 'whitelist'} onclick={() => (authListMode = 'whitelist')}>
                {t('settings.auth.listModeWhitelist')}
              </button>
              <button class:on={authListMode === 'blacklist'} onclick={() => (authListMode = 'blacklist')}>
                {t('settings.auth.listModeBlacklist')}
              </button>
            </div>
            {#if authListMode !== 'none'}
              <p class="tip">{t('settings.auth.listHint')}</p>
              {#if authFolders.length === 0}
                <p class="tip">{t('settings.auth.noFolders')}</p>
              {:else}
                <div class="auth-folders">
                  {#each authFolders as f (f.id)}
                    <label class="folder-check" style="padding-left: {f.depth * 14}px">
                      <input
                        type="checkbox"
                        checked={authFolderList.includes(f.id)}
                        onchange={() => toggleFolderInList(f.id)}
                      />
                      <span>{f.title || t('common.untitled')}</span>
                    </label>
                  {/each}
                </div>
              {/if}
            {/if}
          </div>
        {/if}

        {#if authError}
          <div class="error"><Icon name="alert" size={14} /> {authError}</div>
        {/if}
        <div class="ai-actions">
          {#if authSaved}<span class="saved">{t('settings.auth.saved')}</span>{/if}
          {#if authPasswordSet}
            <Button
              variant="danger"
              label={t('settings.auth.clearPassword')}
              onclick={() => saveAuth(true)}
              disabled={authSaving}
            />
          {/if}
          <Button
            variant="primary"
            label={t('settings.auth.save')}
            onclick={() => saveAuth(false)}
            disabled={authSaving}
          />
        </div>
      </div>
    {/if}

    {#if error}
      <div class="error"><Icon name="alert" size={14} /> {error}</div>
    {/if}

    <div class="actions">
      <Button
        variant="primary"
        label={submitting ? t('settings.connecting') : libMode === 'new' ? t('settings.createConnect') : t('settings.connect')}
        onclick={submit}
        disabled={submitting || (prefillMissing && sourceType === prefillKey)}
      />
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .overlay.setup {
    background: var(--bg);
  }
  .card {
    width: 440px;
    max-width: calc(100vw - 32px);
    /* 内容较多（数据源 + AI + 访问控制段）时卡片自身滚动，避免超出视口后底部按钮不可达 */
    max-height: calc(100vh - 32px);
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 22px 24px 24px;
    box-shadow: var(--shadow-modal);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .hint {
    color: var(--text-dim);
    font-size: 13px;
    margin: 6px 0 16px;
  }
  .seg {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }
  .seg-wrap {
    flex-wrap: wrap;
  }
  .seg-wrap button {
    flex: 1 1 calc(50% - 8px);
    min-width: 120px;
  }
  .plugin-form {
    margin-top: 14px;
  }
  .seg button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  .seg button:active {
    transform: scale(0.98);
  }
  .seg button.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
    color: var(--text-dim);
    margin: 8px 0 4px;
  }
  label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 14px;
  }
  input {
    display: block;
    width: 100%;
    box-sizing: border-box;
    margin-top: 5px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg-side);
    color: var(--text);
    font-size: 13px;
  }
  .tip {
    font-size: 11px;
    color: var(--text-dim);
    margin: 6px 0 0;
  }
  .toggle {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-top: 18px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
    cursor: pointer;
  }
  .toggle input {
    display: block;
    width: 16px;
    height: 16px;
    margin: 1px 0 0;
    flex: 0 0 auto;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .toggle-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .toggle-title {
    font-size: 13px;
    color: var(--text);
    font-weight: 600;
  }
  .toggle-desc {
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.4;
  }
  .ai-section {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }
  .ai-section h3 {
    margin: 0;
    font-size: 14px;
  }
  .ai-section .hint {
    margin: 4px 0 10px;
  }
  .ai-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
    font-size: 13px;
  }
  .ai-field span {
    color: var(--text-dim);
  }
  .ai-field input,
  .ai-field select {
    width: 100%;
    box-sizing: border-box;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }
  .ai-field input:focus,
  .ai-field select:focus {
    outline: none;
    border-color: var(--accent);
  }
  .ai-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }
  .ai-actions .saved {
    color: var(--success);
    font-size: 12px;
  }
  /* 访问控制：黑白名单范围选择 */
  .auth-scope {
    margin-top: 10px;
  }
  .scope-label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  .auth-folders {
    margin-top: 8px;
    max-height: 200px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 6px;
  }
  .folder-check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
    font-size: 13px;
    cursor: pointer;
    border-radius: 6px;
  }
  .folder-check:hover {
    background: var(--hover, rgba(127, 127, 127, 0.1));
  }
  .folder-check input {
    flex: none;
  }
  .error {
    margin-top: 14px;
    padding: 8px 10px;
    background: var(--danger-soft);
    color: var(--danger);
    border-radius: 7px;
    font-size: 12px;
    word-break: break-all;
  }
  .actions {
    margin-top: 20px;
    display: flex;
    justify-content: flex-end;
  }
</style>
