<script lang="ts">
  // 编辑器工具栏：按当前模式渲染已注册命令（分组间加分隔）。
  // 放在笔记工具栏左侧（note-toolbar 位置）；命令通过传入的 handle 操作编辑器。
  import Button from './Button.svelte'
  import { t } from './i18n.svelte'
  import type { MsgKey } from './messages'
  import { commandsForMode } from './editor/commands'
  import type { EditorCommand, EditorHandle, EditorMode } from './editor/types'

  let {
    handle,
    mode,
  }: {
    handle: EditorHandle | null
    mode: EditorMode
  } = $props()

  let commands = $derived(commandsForMode(mode))

  function run(cmd: EditorCommand) {
    if (handle) cmd.run(handle)
  }

  // title 内置为 MsgKey（翻译）；将来插件命令给字面串时 t() 会原样回退，故 cmd.title 保持 string。
  const label = (cmd: EditorCommand) => t(cmd.title as MsgKey)
</script>

{#if commands.length}
  <div class="tools">
    {#each commands as cmd, i (cmd.id)}
      {#if i > 0 && commands[i - 1].group !== cmd.group}<span class="sep"></span>{/if}
      <Button
        variant="ghost"
        iconOnly
        icon={cmd.icon}
        label={label(cmd)}
        onclick={() => run(cmd)}
        disabled={!handle}
      />
    {/each}
  </div>
{/if}

<style>
  .tools {
    display: flex;
    align-items: center;
    gap: 1px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tools::-webkit-scrollbar {
    display: none;
  }
  .sep {
    flex: 0 0 auto;
    width: 1px;
    height: 18px;
    margin: 0 5px;
    background: var(--border);
  }
</style>
