// 为未自带 TS 类型的 markdown-it 插件提供最小声明。
// （@vscode/markdown-it-katex 与 markdown-it-multimd-table 自带类型，无需声明。）

declare module 'markdown-it-task-lists' {
  import { PluginWithOptions } from 'markdown-it'
  const plugin: PluginWithOptions<{ enabled?: boolean; label?: boolean; labelAfter?: boolean }>
  export default plugin
}
declare module 'markdown-it-mark' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-footnote' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-sub' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-sup' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-ins' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-deflist' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-abbr' {
  import { PluginSimple } from 'markdown-it'
  const plugin: PluginSimple
  export default plugin
}
declare module 'markdown-it-emoji' {
  import { PluginSimple } from 'markdown-it'
  export const full: PluginSimple
  export const light: PluginSimple
  export const bare: PluginSimple
}
