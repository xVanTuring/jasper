<script lang="ts">
  // 插件管理面板（顶栏入口，仿 ResourcePanel）：两个 tab——
  // 「已安装」：列表（contributes 徽标 / 错误徽标）· 启停（enable = 能力授权，弹 PluginConsent）
  //            · 安装 .jplug/.zip · 卸载 · 插件设置（SchemaForm 内联展开）。
  // 「市场」：拉 registry 静态索引 → 双语取词 → 兼容过滤 → 浏览器下载 .jplug 校验 sha256 → 装进宿主。
  // readOnly 时仅浏览（服务端 guard_read_only 反正也会硬拦写操作）。
  import { onMount } from 'svelte'
  import { fade, fly, scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { api, type PluginInfo } from './api'
  import { getLocale, t } from './i18n.svelte'
  import Button from './Button.svelte'
  import Icon from './Icon.svelte'
  import SchemaForm from './SchemaForm.svelte'
  import PluginConsent from './PluginConsent.svelte'
  import {
    installPlugin,
    loadPlugins,
    pluginList,
    setPluginEnabled,
    uninstallPlugin,
  } from './plugins.svelte'
  import { pickText, type MarketEntry } from './market'
  import {
    entryState,
    installFromMarket,
    loadMarket,
    marketEntries,
    marketError,
    marketLoading,
  } from './market.svelte'

  let {
    onClose,
    onChanged,
    readOnly = false,
  }: {
    onClose: () => void
    onChanged?: () => void // 启停/卸载可能影响数据源与主题，通知父级刷新
    readOnly?: boolean
  } = $props()

  let tab = $state<'installed' | 'market'>('installed')
  let working = $state(false)
  let error = $state('')
  let fileInput: HTMLInputElement | null = $state(null)
  // 待授权插件（安装后 needs_consent，或对禁用的后端插件点了启用）
  let consentFor = $state<PluginInfo | null>(null)
  // 内联设置展开
  let settingsFor = $state<string | null>(null)
  let settingsValues = $state<Record<string, unknown>>({})
  let secretSet = $state<Record<string, boolean>>({})
  let settingsSavedTick = $state(false)

  onMount(() => {
    loadPlugins()
  })

  async function run(fn: () => Promise<void>) {
    error = ''
    working = true
    try {
      await fn()
      onChanged?.()
    } catch (e) {
      error = e instanceof Error ? e.message : `${e}`
    } finally {
      working = false
    }
  }

  async function pickFile(ev: Event) {
    const input = ev.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = '' // 允许重复选择同一文件
    if (!file) return
    await run(async () => {
      try {
        const r = await installPlugin(file)
        if (r.needs_consent) consentFor = r.plugin
      } catch (e) {
        // 同版本重装：询问是否强制覆盖
        if (e instanceof Error && e.message.includes('version_conflict')) {
          if (confirm(t('plugins.confirmForce'))) {
            const r = await installPlugin(file, true)
            if (r.needs_consent) consentFor = r.plugin
          }
          return
        }
        throw e
      }
    })
  }

  function toggleEnable(p: PluginInfo) {
    if (p.enabled) {
      void run(() => setPluginEnabled(p.id, false))
    } else if (p.has_backend) {
      consentFor = p // enable = 授权动作：先展示能力清单
    } else {
      void run(() => setPluginEnabled(p.id, true))
    }
  }

  function acceptConsent() {
    const p = consentFor
    consentFor = null
    if (p) void run(() => setPluginEnabled(p.id, true))
  }

  async function removePlugin(p: PluginInfo) {
    if (!confirm(t('plugins.confirmUninstall', { name: p.name }))) return
    if (settingsFor === p.id) settingsFor = null
    await run(() => uninstallPlugin(p.id))
  }

  async function toggleSettings(p: PluginInfo) {
    error = ''
    if (settingsFor === p.id) {
      settingsFor = null
      return
    }
    try {
      const s = await api.pluginSettings(p.id)
      // 预填：已存值优先，其余用 schema 默认值
      const values: Record<string, unknown> = {}
      for (const [k, f] of Object.entries(p.settings_schema)) {
        if (s.values[k] !== undefined) values[k] = s.values[k]
        else if (f.default !== undefined && f.type !== 'secret') values[k] = f.default
        else if (f.type === 'bool') values[k] = false
      }
      settingsValues = values
      secretSet = s.secret_set
      settingsFor = p.id
    } catch (e) {
      error = `${e}`
    }
  }

  async function saveSettings(p: PluginInfo) {
    // secret 留空 = 键不提交（服务端语义：缺键不变）
    const values: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(settingsValues)) {
      if (p.settings_schema[k]?.type === 'secret' && (v === '' || v === undefined)) continue
      values[k] = v
    }
    await run(async () => {
      await api.savePluginSettings(p.id, values)
      settingsSavedTick = true
      setTimeout(() => (settingsSavedTick = false), 1500)
    })
  }

  function contribBadges(p: PluginInfo): string[] {
    const out: string[] = []
    if (p.contributes.theme.length) out.push(t('plugins.contrib.themes', { n: p.contributes.theme.length }))
    if (p.contributes.locale?.length) out.push(t('plugins.contrib.locales', { n: p.contributes.locale.length }))
    if (p.contributes.storage.length) out.push(t('plugins.contrib.storage', { n: p.contributes.storage.length }))
    if (p.contributes.sidebar.length) out.push(t('plugins.contrib.sidebar', { n: p.contributes.sidebar.length }))
    if (p.hooks.length) out.push(t('plugins.contrib.hooks', { n: p.hooks.length }))
    return out
  }

  // notes:write 的「写入免确认」开关（宿主托管，spec 0.3 §7/§9.5）
  async function toggleAutoApprove(p: PluginInfo) {
    await run(async () => {
      await api.setPluginAutoApprove(p.id, !p.write_auto_approve)
      await loadPlugins()
    })
  }

  // ---------- 市场 ----------

  function openTab(next: 'installed' | 'market') {
    tab = next
    error = ''
    if (next === 'market') void loadMarket()
  }

  function capLabel(cap: string): string {
    if (cap === 'settings') return t('plugins.cap.settings')
    if (cap === 'host:http') return t('plugins.cap.hostHttp')
    return cap
  }

  async function marketInstall(entry: MarketEntry) {
    await run(async () => {
      try {
        const r = await installFromMarket(entry)
        if (r.needs_consent) consentFor = r.plugin
        tab = 'installed' // 启停/授权都在已安装 tab 里做
      } catch (e) {
        const msg = e instanceof Error ? e.message : `${e}`
        if (msg.includes('sha256 mismatch')) throw new Error(t('plugins.market.shaMismatch'))
        if (msg.includes('version_conflict')) {
          if (confirm(t('plugins.confirmForce'))) {
            const r = await installFromMarket(entry, true)
            if (r.needs_consent) consentFor = r.plugin
            tab = 'installed'
          }
          return
        }
        throw e
      }
    })
  }

  function incompatText(entry: MarketEntry, reason: 'api' | 'host'): string {
    return reason === 'api'
      ? t('plugins.market.incompatibleApi', { v: entry.apiVersion })
      : t('plugins.market.incompatibleHost', { v: entry.minHostVersion })
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !consentFor && onClose()} />

<div
  class="overlay"
  role="presentation"
  transition:fade={{ duration: 150 }}
  onclick={(e) => e.target === e.currentTarget && onClose()}
>
  <div class="card" transition:scale={{ duration: 180, start: 0.96, opacity: 0, easing: cubicOut }}>
    <header>
      <h2>{t('plugins.title')}</h2>
      <Button variant="ghost" iconOnly icon="close" label={t('common.close')} onclick={onClose} />
    </header>

    <div class="tabs" role="tablist">
      <button
        role="tab"
        class="tab"
        class:active={tab === 'installed'}
        aria-selected={tab === 'installed'}
        onclick={() => openTab('installed')}
      >
        {t('plugins.tab.installed')}
      </button>
      <button
        role="tab"
        class="tab"
        class:active={tab === 'market'}
        aria-selected={tab === 'market'}
        onclick={() => openTab('market')}
      >
        {t('plugins.tab.market')}
      </button>
    </div>

    {#if tab === 'installed' && !readOnly}
      <div class="bar">
        <input
          type="file"
          accept=".jplug,.zip,application/zip"
          class="file-input"
          bind:this={fileInput}
          onchange={pickFile}
        />
        <Button
          variant="default"
          icon="plus"
          label={working ? t('plugins.installing') : t('plugins.install')}
          onclick={() => fileInput?.click()}
          disabled={working}
        />
      </div>
    {/if}

    {#if error}<div class="error"><Icon name="alert" size={14} /> {error}</div>{/if}

    {#if tab === 'market'}
      {#if marketLoading()}
        <div class="empty">{t('plugins.market.loading')}</div>
      {:else if marketError()}
        <div class="error"><Icon name="alert" size={14} /> {t('plugins.market.error', { msg: marketError() })}</div>
        <div class="bar retry">
          <Button variant="default" label={t('plugins.market.retry')} onclick={() => loadMarket(true)} />
        </div>
      {:else if marketEntries().length === 0}
        <div class="empty">{t('plugins.market.empty')}</div>
      {:else}
        <ul class="list">
          {#each marketEntries() as entry, i (entry.id)}
            {@const st = entryState(entry, pluginList())}
            <li class="row" in:fly={{ y: 6, duration: 220, delay: Math.min(i * 16, 220), easing: cubicOut }}>
              <div class="icon"><Icon name="plug" size={18} /></div>
              <div class="info">
                <div class="title-line">
                  <span class="name" title={entry.id}>{pickText(entry.name, getLocale())}</span>
                  <span class="version">v{entry.version}</span>
                  {#each entry.capabilities as cap (cap)}
                    <span class="badge" title={cap}>{capLabel(cap)}</span>
                  {/each}
                </div>
                <div class="desc" title={pickText(entry.description, getLocale())}>
                  {pickText(entry.description, getLocale())}
                </div>
                <div class="meta">
                  {#if entry.author}<span>{entry.author}</span>{/if}
                  {#if entry.repo}
                    <a href={entry.repo} target="_blank" rel="noopener noreferrer">{t('plugins.market.repo')}</a>
                  {/if}
                </div>
              </div>
              <div class="actions">
                {#if st.kind === 'incompatible'}
                  <span class="state" title={incompatText(entry, st.compat.reason)}>
                    {incompatText(entry, st.compat.reason)}
                  </span>
                {:else if st.kind === 'installed'}
                  <span class="state">{t('plugins.market.installed')}</span>
                {:else if readOnly}
                  <span class="state">{t('plugins.market.readOnly')}</span>
                {:else if st.kind === 'update'}
                  <Button
                    variant="primary"
                    label={working ? t('plugins.market.installing') : t('plugins.market.update', { v: entry.version })}
                    onclick={() => marketInstall(entry)}
                    disabled={working}
                  />
                {:else}
                  <Button
                    variant="default"
                    icon="plus"
                    label={working ? t('plugins.market.installing') : t('plugins.market.install')}
                    onclick={() => marketInstall(entry)}
                    disabled={working}
                  />
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if pluginList().length === 0}
      <div class="empty">{t('plugins.empty')}</div>
    {:else}
      <ul class="list">
        {#each pluginList() as p, i (p.id)}
          <li class="row" in:fly={{ y: 6, duration: 220, delay: Math.min(i * 16, 220), easing: cubicOut }}>
            <div class="icon"><Icon name="plug" size={18} /></div>
            <div class="info">
              <div class="title-line">
                <span class="name" title={p.id}>{p.name}</span>
                <span class="version">v{p.version}</span>
                {#if p.error}
                  <span class="badge badge-error" title={p.error}>{t('plugins.errorBadge')}</span>
                {/if}
                {#each contribBadges(p) as b (b)}
                  <span class="badge">{b}</span>
                {/each}
              </div>
              {#if p.description}<div class="desc">{p.description}</div>{/if}
              {#if p.enabled && !readOnly && p.capabilities.includes('notes:write')}
                <!-- 宿主托管的写入免确认（不进插件自身 settings，防插件自改绕过确认，spec 0.3 §7） -->
                <label class="aa-toggle" title={t('plugins.autoApproveDesc')}>
                  <input
                    type="checkbox"
                    checked={p.write_auto_approve}
                    disabled={working}
                    onchange={(e) => {
                      e.currentTarget.checked = p.write_auto_approve
                      void toggleAutoApprove(p)
                    }}
                  />
                  <span>{t('plugins.autoApprove')}</span>
                </label>
              {/if}
            </div>
            <div class="actions">
              {#if Object.keys(p.settings_schema).length > 0}
                <Button
                  variant="ghost"
                  iconOnly
                  icon="settings"
                  label={t('plugins.settings')}
                  onclick={() => toggleSettings(p)}
                />
              {/if}
              {#if !readOnly}
                <label class="switch" title={p.enabled ? t('plugins.enabled') : t('plugins.disabled')}>
                  <input
                    type="checkbox"
                    checked={p.enabled}
                    disabled={working || !!p.error}
                    onchange={(e) => {
                      // 视觉状态由数据驱动：先复位原生翻转（consent 被拒/请求失败时不留假开启态），
                      // 真正的启停成功后 loadPlugins 刷新 p.enabled 再更新
                      e.currentTarget.checked = p.enabled
                      toggleEnable(p)
                    }}
                  />
                  <span class="track"><span class="knob"></span></span>
                </label>
                <Button
                  variant="danger"
                  iconOnly
                  icon="trash"
                  label={t('plugins.uninstall')}
                  onclick={() => removePlugin(p)}
                  disabled={working}
                />
              {:else}
                <span class="state">{p.enabled ? t('plugins.enabled') : t('plugins.disabled')}</span>
              {/if}
            </div>
            {#if settingsFor === p.id}
              <div class="settings">
                <SchemaForm schema={p.settings_schema} bind:values={settingsValues} {secretSet} />
                {#if !readOnly}
                  <div class="settings-actions">
                    {#if settingsSavedTick}<span class="saved">{t('plugins.settingsSaved')}</span>{/if}
                    <Button variant="primary" label={t('common.save')} onclick={() => saveSettings(p)} disabled={working} />
                  </div>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

{#if consentFor}
  <PluginConsent plugin={consentFor} onAccept={acceptConsent} onKeepDisabled={() => (consentFor = null)} />
{/if}

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
  .card {
    width: 560px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 18px 20px 16px;
    box-shadow: var(--shadow-modal);
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
  .tabs {
    display: flex;
    gap: 4px;
    margin-top: 10px;
    border-bottom: 1px solid var(--border);
  }
  .tab {
    appearance: none;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 6px 10px;
    font-size: 13px;
    color: var(--text-dim);
    cursor: pointer;
  }
  .tab.active {
    color: var(--text);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
  .bar {
    display: flex;
    justify-content: flex-end;
    margin: 12px 0 6px;
  }
  .bar.retry {
    justify-content: center;
  }
  .meta {
    display: flex;
    gap: 10px;
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .meta a {
    color: var(--accent);
    text-decoration: none;
  }
  .meta a:hover {
    text-decoration: underline;
  }
  .file-input {
    display: none;
  }
  .error {
    margin-top: 10px;
    padding: 8px 10px;
    background: var(--danger-soft);
    color: var(--danger);
    border-radius: 7px;
    font-size: 12px;
    word-break: break-all;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .empty {
    padding: 36px 0;
    text-align: center;
    color: var(--text-dim);
    font-size: 13px;
  }
  .list {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px 6px;
    border-top: 1px solid var(--border);
  }
  .icon {
    color: var(--text-dim);
    display: flex;
  }
  .info {
    min-width: 0;
  }
  .title-line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .name {
    font-size: 14px;
    font-weight: 600;
  }
  .version {
    font-size: 11px;
    color: var(--text-dim);
  }
  .badge {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .badge-error {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .desc {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .aa-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-dim);
    cursor: pointer;
  }
  .aa-toggle input {
    width: 13px;
    height: 13px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .state {
    font-size: 12px;
    color: var(--text-dim);
  }
  /* 开关（纯 CSS，跟随主题令牌） */
  .switch {
    position: relative;
    display: inline-flex;
    cursor: pointer;
  }
  .switch input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .track {
    width: 34px;
    height: 20px;
    border-radius: 999px;
    background: var(--border);
    display: inline-flex;
    align-items: center;
    padding: 2px;
    box-sizing: border-box;
    transition: background 0.15s ease;
  }
  .knob {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg);
    transition: transform 0.15s ease;
  }
  .switch input:checked + .track {
    background: var(--accent);
  }
  .switch input:checked + .track .knob {
    transform: translateX(14px);
  }
  .switch input:disabled + .track {
    opacity: 0.5;
  }
  .settings {
    grid-column: 1 / -1;
    padding: 10px 6px 2px 30px;
    border-top: 1px dashed var(--border);
    margin-top: 8px;
  }
  .settings-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
  }
  .saved {
    font-size: 12px;
    color: var(--success);
  }
</style>
