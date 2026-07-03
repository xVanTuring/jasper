<script lang="ts">
  // 登录框（访问鉴权）：输入访问密码 → api.login → 成功回调。套用 PluginConsent 的模态骨架。
  import { fade, scale } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import Button from './Button.svelte'
  import { t } from './i18n.svelte'
  import { api } from './api'

  let {
    onSuccess,
    onClose,
  }: {
    onSuccess: () => void
    onClose: () => void
  } = $props()

  let password = $state('')
  let submitting = $state(false)
  let error = $state('')

  async function submit(e?: Event) {
    e?.preventDefault()
    if (submitting || !password) return
    submitting = true
    error = ''
    try {
      if (await api.login(password)) {
        onSuccess()
      } else {
        error = t('auth.wrongPassword')
      }
    } catch (err) {
      error = `${err}`
    } finally {
      submitting = false
    }
  }
</script>

<div class="overlay" role="presentation" transition:fade={{ duration: 120 }}>
  <div class="card" transition:scale={{ duration: 170, start: 0.95, opacity: 0, easing: cubicOut }}>
    <h3>{t('auth.dialogTitle')}</h3>
    <p class="intro">{t('auth.dialogDesc')}</p>
    <form onsubmit={submit}>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="pw"
        type="password"
        autocomplete="current-password"
        placeholder={t('auth.passwordPlaceholder')}
        bind:value={password}
        autofocus
      />
      {#if error}
        <p class="err">{error}</p>
      {/if}
      <div class="actions">
        <Button variant="default" label={t('common.cancel')} onclick={onClose} />
        <Button
          variant="primary"
          type="submit"
          label={submitting ? t('auth.submitting') : t('auth.submit')}
          disabled={submitting || !password}
        />
      </div>
    </form>
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
    z-index: 140; /* 登录闸门盖在一切之上 */
  }
  .card {
    width: 380px;
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
  .intro {
    margin: 0 0 14px;
    font-size: 13px;
    color: var(--text-dim);
  }
  .pw {
    width: 100%;
    box-sizing: border-box;
    padding: 8px 10px;
    font-size: 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text);
  }
  .pw:focus {
    outline: none;
    border-color: var(--accent, #4a90d9);
  }
  .err {
    margin: 8px 0 0;
    font-size: 13px;
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
