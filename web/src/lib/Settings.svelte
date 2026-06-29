<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from './api'

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
        error = res.error || '连接失败'
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
      <h2>{mode === 'setup' ? '欢迎使用 joplin-lite' : '设置'}</h2>
      {#if mode === 'settings'}
        <button class="x" onclick={() => onClose?.()} aria-label="关闭">✕</button>
      {/if}
    </header>

    {#if mode === 'setup'}
      <p class="hint">首次使用，请选择笔记库。</p>
    {/if}

    <!-- 现有 / 新建 -->
    <div class="seg">
      <button class:on={libMode === 'existing'} onclick={() => (libMode = 'existing')}>
        使用现有笔记库
      </button>
      <button class:on={libMode === 'new'} onclick={() => (libMode = 'new')}>
        新建笔记库
      </button>
    </div>
    <p class="sub">
      {libMode === 'existing'
        ? '连接到已有的 Joplin 同步目录 / WebDAV，读取并编辑已有笔记。'
        : '在指定位置创建一个空的新库（会写入 info.json）。'}
    </p>

    <!-- 本地 / WebDAV -->
    <div class="seg">
      <button class:on={sourceType === 'local'} onclick={() => (sourceType = 'local')}>
        📁 本地文件夹
      </button>
      <button class:on={sourceType === 'webdav'} onclick={() => (sourceType = 'webdav')}>
        ☁️ WebDAV
      </button>
    </div>

    {#if sourceType === 'local'}
      <label>
        文件夹路径
        <input
          type="text"
          bind:value={localPath}
          placeholder={libMode === 'new' ? '/Users/你/新笔记库' : '/Users/你/Joplin同步目录'}
        />
      </label>
      <p class="tip">服务运行在本机，请填写该机器上的绝对路径。</p>
    {:else}
      <label>
        WebDAV 地址
        <input type="text" bind:value={webdavUrl} placeholder="https://host/remote.php/dav/files/用户/Joplin" />
      </label>
      <label>
        用户名
        <input type="text" bind:value={webdavUser} autocomplete="username" />
      </label>
      <label>
        密码
        <input type="password" bind:value={webdavPass} autocomplete="current-password" />
      </label>
    {/if}

    {#if error}
      <div class="error">⚠️ {error}</div>
    {/if}

    <div class="actions">
      <button class="primary" onclick={submit} disabled={submitting}>
        {submitting ? '连接中…' : libMode === 'new' ? '创建并连接' : '连接'}
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
