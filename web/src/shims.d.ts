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

// wasm demo 模块：由 `pnpm build:wasm`（wasm-pack）产出到 src/wasm-pkg（已 gitignore）。
// 非 demo 的类型检查/构建时该目录不存在，api.ts 里 `import('../wasm-pkg/jasper_wasm.js')`
// 会让 tsc 报「找不到模块」。这里给个兜底声明；真存在时（build:demo）相对解析仍优先用真实 .d.ts。
declare module '*wasm-pkg/jasper_wasm.js' {
  export default function init(input?: unknown): Promise<unknown>
  export class Demo {
    folders(): string
    notes(folder: string): string
    note(id: string): string
    search(query: string): string
  }
}
