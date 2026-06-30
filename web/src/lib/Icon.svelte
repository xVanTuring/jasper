<script lang="ts">
  // 语义图标：按名字渲染 icons.css 里的 --icon-<name>（CSS mask + currentColor）。
  // 单色、跟随文字色；主题可通过覆盖 --icon-<name> 换图标。未知名字则不显示。
  let { name, size = 16 }: { name: string; size?: number } = $props()
</script>

<i
  class="icon"
  style="--icon-src: var(--icon-{name}); --icon-size: {size}px"
  aria-hidden="true"
></i>

<style>
  .icon {
    display: inline-block;
    flex: 0 0 auto;
    width: var(--icon-size);
    height: var(--icon-size);
    background-color: currentColor;
    -webkit-mask-image: var(--icon-src);
    mask-image: var(--icon-src);
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
    -webkit-mask-position: center;
    mask-position: center;
    -webkit-mask-size: contain;
    mask-size: contain;
  }
</style>
