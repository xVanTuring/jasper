<script lang="ts">
  // 能力授权确认（spec §5/§7）：含 [backend] 的插件启用前展示其申请的能力。
  // host:http 必须附带显式联网警告（spec §7，0.2）。
  import { fade, scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import Button from './Button.svelte'
  import Icon from './Icon.svelte'
  import { t } from './i18n.svelte'
  import type { PluginInfo } from './api'

  let {
    plugin,
    onAccept,
    onKeepDisabled,
  }: {
    plugin: PluginInfo
    onAccept: () => void
    onKeepDisabled: () => void
  } = $props()

  function capLabel(cap: string): string {
    switch (cap) {
      case 'notes:read':
        return t('plugins.cap.notesRead')
      case 'notes:write':
        return t('plugins.cap.notesWrite')
      case 'host:ai':
        return t('plugins.cap.hostAi')
      case 'settings':
        return t('plugins.cap.settings')
      case 'host:http':
        return t('plugins.cap.hostHttp')
      default:
        return cap
    }
  }
</script>

<div class="overlay" role="presentation" transition:fade={{ duration: 120 }}>
  <div class="card" transition:scale={{ duration: 170, start: 0.95, opacity: 0, easing: cubicOut }}>
    <h3>{t('plugins.consent.title', { name: plugin.name })}</h3>
    <p class="intro">{t('plugins.consent.intro')}</p>
    {#if plugin.capabilities.length === 0}
      <p class="none">{t('plugins.consent.none')}</p>
    {:else}
      <ul class="caps">
        {#each plugin.capabilities as cap (cap)}
          <li class:warn={cap === 'host:http'}>
            <Icon name={cap === 'host:http' ? 'globe' : 'check'} size={14} />
            <span>
              {capLabel(cap)}
              {#if cap === 'host:http'}
                <span class="warn-line">{t('plugins.cap.hostHttpWarn')}</span>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <Button variant="default" label={t('plugins.consent.keepDisabled')} onclick={onKeepDisabled} />
      <Button variant="primary" label={t('plugins.consent.accept')} onclick={onAccept} />
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
    z-index: 120; /* 盖在插件面板之上 */
  }
  .card {
    width: 400px;
    max-width: calc(100vw - 32px);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 20px 22px;
    box-shadow: var(--shadow-modal);
  }
  h3 {
    margin: 0 0 6px;
    font-size: 16px;
  }
  .intro,
  .none {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--text-dim);
  }
  .caps {
    margin: 0 0 14px;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .caps li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
  }
  .caps li.warn {
    color: var(--danger);
  }
  .warn-line {
    display: block;
    font-size: 12px;
    color: var(--danger);
    margin-top: 2px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
