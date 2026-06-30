<script lang="ts">
  // 主题选择器：调色板按钮 + 下拉菜单，列出内置(auto/light/dark) + 自定义主题。
  import Icon from './Icon.svelte'
  import { t } from './i18n.svelte'
  import { getTheme, setTheme, themeIds, customThemeName, type ThemeSetting } from './theme.svelte'

  let open = $state(false)

  function nameOf(id: ThemeSetting): string {
    if (id === 'auto') return t('theme.auto')
    if (id === 'light') return t('theme.light')
    if (id === 'dark') return t('theme.dark')
    return customThemeName(id) ?? id
  }
  function choose(id: ThemeSetting) {
    setTheme(id)
    open = false
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="picker">
  <button
    class="trigger"
    onclick={() => (open = !open)}
    title={t('theme.pick', { mode: nameOf(getTheme()) })}
    aria-label={t('theme.pick', { mode: nameOf(getTheme()) })}
    aria-haspopup="menu"
    aria-expanded={open}
  >
    <Icon name="palette" size={15} />
  </button>

  {#if open}
    <div class="backdrop" role="presentation" onclick={() => (open = false)}></div>
    <ul class="menu" role="menu">
      {#each themeIds() as id (id)}
        <li role="none">
          <button
            role="menuitemradio"
            aria-checked={getTheme() === id}
            class:active={getTheme() === id}
            onclick={() => choose(id)}
          >
            {nameOf(id)}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    display: inline-flex;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    border-radius: 6px;
    padding: 5px;
    cursor: pointer;
    color: var(--text-dim);
  }
  .trigger:hover {
    background: var(--hover);
    color: var(--text);
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 51;
    min-width: 160px;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow-modal);
  }
  .menu li {
    margin: 0;
  }
  .menu button {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-radius: 6px;
    padding: 7px 10px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu button.active {
    color: var(--accent);
    font-weight: 600;
  }
</style>
