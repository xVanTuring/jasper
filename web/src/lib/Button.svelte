<script lang="ts">
  // 统一按钮：图标 + 文字。图标走 Icon（可换主题），文字走 i18n。
  // 是否显示图标/文字由全局 getButtonDisplay() 控制（无图标必显文字；iconOnly 强制仅图标）。
  import Icon from './Icon.svelte'
  import { getButtonDisplay } from './ui.svelte'

  let {
    icon,
    label = '',
    title,
    onclick,
    variant = 'default',
    size = 15,
    disabled = false,
    iconOnly = false,
    type = 'button',
  }: {
    icon?: string
    label?: string
    title?: string
    onclick?: (e: MouseEvent) => void
    variant?: 'default' | 'ghost' | 'danger' | 'primary'
    size?: number
    disabled?: boolean
    iconOnly?: boolean
    type?: 'button' | 'submit'
  } = $props()

  let showIcon = $derived(!!icon && (iconOnly || getButtonDisplay() !== 'text'))
  let showLabel = $derived(!!label && !iconOnly && (!icon || getButtonDisplay() !== 'icon'))
</script>

<button
  class="btn v-{variant}"
  class:only-icon={showIcon && !showLabel}
  {type}
  {disabled}
  {onclick}
  title={title ?? label}
  aria-label={label || title}
>
  {#if showIcon && icon}<Icon name={icon} {size} />{/if}
  {#if showLabel}<span class="label">{label}</span>{/if}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid transparent;
    border-radius: 6px;
    font-size: 12px;
    line-height: 1;
    color: var(--text);
    background: none;
    cursor: pointer;
    padding: 5px 10px;
    white-space: nowrap;
    transition: background 0.13s ease, color 0.13s ease, border-color 0.13s ease,
      transform 0.08s ease, filter 0.13s ease;
  }
  .btn:active:not(:disabled) {
    transform: scale(0.95);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .btn.only-icon {
    padding: 5px;
  }
  .v-default {
    border-color: var(--border);
  }
  .v-default:hover:not(:disabled) {
    background: var(--hover);
  }
  .v-ghost {
    color: var(--text-dim);
  }
  .v-ghost:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .v-danger {
    border-color: var(--border);
  }
  .v-danger:hover:not(:disabled) {
    background: var(--danger);
    color: var(--on-accent);
    border-color: var(--danger);
  }
  .v-primary {
    background: var(--accent);
    color: var(--on-accent);
    padding: 9px 18px;
    font-size: 14px;
  }
  .v-primary:hover:not(:disabled) {
    filter: brightness(1.06);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
