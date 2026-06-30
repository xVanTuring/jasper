<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from './api'
  import { t } from './i18n.svelte'

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
  let sourceType = $state<'local' | 'webdav'>('local')
  let localPath = $state('')
  let webdavUrl = $state('')
  let webdavUser = $state('')
  let webdavPass = $state('')

  let submitting = $state(false)
  let error = $state('')

  onMount(async () => {
    // 设置模式下预填当前配置
    if (mode === 'settings') {
      try {
        const c = await api.getConfig()
        if (c.source_type === 'webdav') sourceType = 'webdav'
        else if (c.source_type === 'local') sourceType = 'local'
        localPath = c.local_path
        webdavUrl = c.webdav_url
        webdavUser = c.webdav_user
        webdavPass = c.webdav_pass
      } catch {
        /* 忽略 */
      }
    }
  })

  async function submit() {
    error = ''
    submitting = true
    try {
      const res = await api.saveConfig({
        source_type: sourceType,
        local_path: localPath.trim(),
        webdav_url: webdavUrl.trim(),
        webdav_user: webdavUser,
        webdav_pass: webdavPass,
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

<div class="overlay" class:setup={mode === 'setup'}>
  <div class="card">
    <header>
      <h2>{mode === 'setup' ? t('settings.welcomeTitle') : t('settings.title')}</h2>
      {#if mode === 'settings'}
        <button class="x" onclick={() => onClose?.()} aria-label={t('common.close')}>✕</button>
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

    <!-- 本地 / WebDAV -->
    <div class="seg">
      <button class:on={sourceType === 'local'} onclick={() => (sourceType = 'local')}>
        {t('settings.local')}
      </button>
      <button class:on={sourceType === 'webdav'} onclick={() => (sourceType = 'webdav')}>
        {t('settings.webdav')}
      </button>
    </div>

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
    {:else}
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
    {/if}

    {#if error}
      <div class="error">⚠️ {error}</div>
    {/if}

    <div class="actions">
      <button class="primary" onclick={submit} disabled={submitting}>
        {submitting ? t('settings.connecting') : libMode === 'new' ? t('settings.createConnect') : t('settings.connect')}
      </button>
    </div>
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
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
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
  .x {
    background: none;
    border: none;
    font-size: 16px;
    color: var(--text-dim);
    cursor: pointer;
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
  .seg button {
    flex: 1;
    padding: 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-side);
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
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
  .error {
    margin-top: 14px;
    padding: 8px 10px;
    background: rgba(192, 57, 43, 0.12);
    color: #c0392b;
    border-radius: 7px;
    font-size: 12px;
    word-break: break-all;
  }
  .actions {
    margin-top: 20px;
    display: flex;
    justify-content: flex-end;
  }
  .primary {
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 8px;
    padding: 9px 20px;
    font-size: 14px;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
