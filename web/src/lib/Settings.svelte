<script lang="ts">
  import { onMount } from 'svelte'
  import { fade, scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { api, type AiConfig } from './api'
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
  }: {
    mode: 'setup' | 'settings'
    onDone: () => void
    onClose?: () => void
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
