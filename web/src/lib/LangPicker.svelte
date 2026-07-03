<script lang="ts">
  // 语言选择器：短标签触发 + 下拉菜单，列出内置(中/EN) + 插件贡献的语言。
  // 只有内置两门语言时也用下拉（统一交互）；插件语言包加载后自动出现在列表里。
  import { scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { t, getLocale, setLocale, availableLocales, localeName } from './i18n.svelte'

  let open = $state(false)

  // 触发按钮上的短标签：内置 zh→中、en→EN，插件语言取 code 前两位大写。
  function shortLabel(code: string): string {
    if (code === 'zh') return '中'
    if (code === 'en') return 'EN'
    return code.slice(0, 2).toUpperCase()
  }
  function choose(code: string) {
    setLocale(code)
    open = false
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="picker">
  <button
    class="trigger"
    onclick={() => (open = !open)}
    title={t('common.langTitle')}
    aria-label={t('common.langTitle')}
    aria-haspopup="menu"
    aria-expanded={open}
  >
    {shortLabel(getLocale())}
  </button>

  {#if open}
    <div class="backdrop" role="presentation" onclick={() => (open = false)}></div>
    <ul class="menu" role="menu" transition:scale={{ duration: 140, start: 0.92, opacity: 0, easing: cubicOut }}>
      {#each availableLocales() as loc (loc.code)}
        <li role="none">
          <button
            role="menuitemradio"
            aria-checked={getLocale() === loc.code}
            class:active={getLocale() === loc.code}
            onclick={() => choose(loc.code)}
          >
            {localeName(loc.code)}
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
    min-width: 30px;
    height: 30px;
    padding: 0 8px;
    background: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 13px;
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
    transform-origin: top right;
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
