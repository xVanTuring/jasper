// 后端 API 客户端。开发期经 Vite 代理到 27583，生产期同源访问。
import { t } from './i18n.svelte'
import type { Schema } from './schema'

// 拖拽（移动）用的 dataTransfer MIME：笔记 / 笔记本各一种，放置目标据此区分。
export const NOTE_DND_TYPE = 'application/x-jasper-note-id'
export const FOLDER_DND_TYPE = 'application/x-jasper-folder-id'

export interface FolderNode {
  id: string
  title: string
  note_count: number
  children: FolderNode[]
}

export interface NoteSummary {
  id: string
  title: string
  updated_time: number
  parent_id: string
  is_todo: boolean
  todo_completed: boolean
  task_done: number // 正文任务清单已完成数
  task_total: number // 正文任务清单总数（0 = 无任务清单）
}

export interface FolderRef {
  id: string
  title: string
  parent_id: string
}

export interface NoteDetail {
  id: string
  title: string
  body: string
  markup_language: number // 1=Markdown, 2=HTML
  parent_id: string
  created_time: number
  updated_time: number
  source_url: string
  is_todo: boolean
  todo_completed: boolean
}

export interface SourceConfig {
  source_type: string // 'local' | 'webdav' | 'plugin'
  local_path: string
  webdav_url: string
  webdav_user: string
  webdav_pass: string
  // source_type === 'plugin'（插件存储 provider，spec 0.2）
  plugin_id: string
  plugin_storage: string
  plugin_config: string // 服务端存 JSON 文本（GET /api/config 原样回显，含 secret，与 webdav_pass 同姿势）
  read_only: boolean // 只读模式：拒绝一切写操作
}

export interface StatusResp {
  configured: boolean
  source_type: string
  notes: number
  folders: number
  read_only: boolean // 服务端只读模式是否开启
  version: string // 服务端版本（市场 UI 做 minHostVersion 兼容过滤）
}

export interface ConfigResult {
  ok: boolean
  error: string | null
  notes: number
  folders: number
}

// PUT /api/config 的请求体：plugin_config 以对象提交（服务端校验后规范化存储）。
export interface ApplyConfigReq extends Omit<SourceConfig, 'plugin_config'> {
  plugin_config: Record<string, unknown>
  create_new: boolean
}

// ---------- 插件（服务端 --features plugins；未编译时探测为不可用）----------

export interface ThemeContribution {
  id: string
  name: string
  base: 'light' | 'dark'
  css: string // 包内相对路径，经 pluginAssetUrl 取
}

export interface StorageContribution {
  id: string
  name: string
  icon: string // 图标令牌名（--icon-*），空 = 用默认 plug
  config_schema: Schema
}

export interface CommandContribution {
  id: string
  title: string
  icon: string // 图标令牌名，空 = 默认
  target: 'backend' | 'builtin'
}

export interface ToolbarContribution {
  command: string
  location: 'note-toolbar' | 'topbar'
}

export interface PluginContributes {
  theme: ThemeContribution[]
  storage: StorageContribution[]
  command: CommandContribution[]
  toolbar: ToolbarContribution[]
}

export interface PluginInfo {
  id: string
  name: string
  version: string
  api_version: string
  description: string
  author: string
  enabled: boolean
  has_backend: boolean
  capabilities: string[]
  hooks: string[]
  error: string | null
  contributes: PluginContributes
  settings_schema: Schema
}

export interface PluginsResp {
  host: { version: string; api_versions: string[] }
  plugins: PluginInfo[]
}

export interface PluginInstallResult {
  plugin: PluginInfo
  needs_consent: boolean
}

export interface PluginSettingsResp {
  values: Record<string, unknown>
  secret_set: Record<string, boolean> // secret 不回显，仅标记「已设置」
}

export interface ResourceUpload {
  id: string
  title: string
  mime: string
  file_extension: string
  size: number
  markdown: string // 可直接插入正文的引用片段
}

export interface ResourceInfo {
  id: string
  title: string
  mime: string
  file_extension: string
  size: number
  updated_time: number
  used_by: number // 引用该资源的笔记数（0 = 孤儿）
}

// 「demo 模式」：构建时 VITE_DEMO=1，则只读查询走浏览器内的 WASM（jasper-core 编译产物），
// 不需要任何后端 server——用于纯静态演示站点。
const DEMO = import.meta.env.VITE_DEMO === '1'
/// 是否为浏览器内 WASM 演示构建（只读）。供 UI 提示/禁用写入用。
export const IS_DEMO = DEMO

let _demo: Promise<{ folders(): string; notes(f: string): string; note(id: string): string; search(q: string): string }> | null = null
function wasmDemo() {
  if (!_demo) {
    _demo = (async () => {
      const mod = await import('../wasm-pkg/jasper_wasm.js')
      await mod.default() // 加载并初始化 .wasm
      return new mod.Demo()
    })()
  }
  return _demo
}

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`${url} -> ${res.status}`)
  return res.json() as Promise<T>
}

async function sendJson<T>(url: string, method: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${method} ${url} -> ${res.status}`)
  return res.json() as Promise<T>
}

// Joplin 内部资源/笔记链接 `:/<32hex>` 或 `joplin://<32hex>` → 解析出 id（无则 null）。
// 与 render.ts 的改写逻辑同源，供富文本编辑器把 :/id 映射成可显示 URL。
const RESOURCE_LINK = /^(?:joplin:\/\/|:\/)([0-9a-zA-Z]{32})(?:[#?].*)?$/
export function parseResourceId(url: string): string | null {
  const m = RESOURCE_LINK.exec((url || '').trim())
  return m ? m[1] : null
}

// 统计 markdown 任务清单（GFM checkbox）完成/总数 [done, total]。与 core::library::count_tasks 同义，
// 供编辑时按当前正文实时显示进度（列表项的计数由后端给）。
export function taskProgress(body: string): [number, number] {
  let done = 0
  let total = 0
  for (const line of (body || '').split('\n')) {
    const m = /^\s*[-*+] \[([ xX])\](?:\s|$)/.exec(line)
    if (!m) continue
    total++
    if (m[1] !== ' ') done++
  }
  return [done, total]
}

const httpApi = {
  status: () => getJson<StatusResp>('/api/status'),
  getConfig: () => getJson<SourceConfig>('/api/config'),
  saveConfig: (data: ApplyConfigReq) => sendJson<ConfigResult>('/api/config', 'PUT', data),

  folders: () => getJson<FolderNode[]>('/api/folders'),
  createFolder: (data: { parent_id: string; title: string }) =>
    sendJson<FolderRef>('/api/folders', 'POST', data),
  renameFolder: (id: string, title: string) =>
    sendJson<FolderRef>(`/api/folders/${id}`, 'PUT', { title }),
  moveFolder: (id: string, parentId: string) =>
    sendJson<FolderRef>(`/api/folders/${id}/move`, 'PUT', { parent_id: parentId }),
  notes: (folderId: string) =>
    getJson<NoteSummary[]>(`/api/notes?folder=${encodeURIComponent(folderId)}`),
  note: (id: string) => getJson<NoteDetail>(`/api/notes/${id}`),
  search: (q: string) => getJson<NoteSummary[]>(`/api/search?q=${encodeURIComponent(q)}`),
  resourceUrl: (id: string) => `/api/resources/${id}`,

  // 写入
  updateNote: (id: string, data: { title: string; body: string }) =>
    sendJson<NoteDetail>(`/api/notes/${id}`, 'PUT', data),
  // 移动笔记到另一个笔记本（改 parent_id）
  moveNote: (id: string, parentId: string) =>
    sendJson<NoteDetail>(`/api/notes/${id}/move`, 'PUT', { parent_id: parentId }),
  createNote: (data: { parent_id: string; title?: string; body?: string; is_todo?: boolean }) =>
    sendJson<NoteDetail>('/api/notes', 'POST', data),
  deleteNote: async (id: string) => {
    const res = await fetch(`/api/notes/${id}`, { method: 'DELETE' })
    if (!res.ok) throw new Error(`DELETE -> ${res.status}`)
  },

  // 上传资源（图片/附件）：原始二进制作请求体，Content-Type=文件 MIME，文件名走 query。
  uploadResource: async (file: Blob, filename: string): Promise<ResourceUpload> => {
    const res = await fetch(`/api/resources?filename=${encodeURIComponent(filename)}`, {
      method: 'POST',
      headers: { 'Content-Type': file.type || 'application/octet-stream' },
      body: file,
    })
    if (!res.ok) throw new Error(`${t('api.uploadFailed')} -> ${res.status}`)
    return res.json() as Promise<ResourceUpload>
  },

  // 资源管理
  resources: () => getJson<ResourceInfo[]>('/api/resources'),
  renameResource: (id: string, title: string) =>
    sendJson<ResourceInfo>(`/api/resources/${id}`, 'PUT', { title }),
  deleteResource: async (id: string) => {
    const res = await fetch(`/api/resources/${id}`, { method: 'DELETE' })
    if (!res.ok) throw new Error(`${t('api.deleteResFailed')} -> ${res.status}`)
  },

  // ---------- 插件 ----------
  // 探测坑：服务端未编译 plugins feature 时路由不存在，但 SPA fallback 会对
  // GET /api/plugins 回 200 的 index.html —— 必须校验 content-type 才能判定不可用。
  plugins: async (): Promise<PluginsResp | null> => {
    try {
      const res = await fetch('/api/plugins')
      const ct = res.headers.get('content-type') ?? ''
      if (!res.ok || !ct.includes('application/json')) return null
      return (await res.json()) as PluginsResp
    } catch {
      return null
    }
  },
  // 安装 .jplug/.zip：原始二进制作请求体（同资源上传惯例）。失败抛带服务端 message 的 Error。
  installPlugin: async (file: Blob, force = false): Promise<PluginInstallResult> => {
    const res = await fetch(`/api/plugins/install${force ? '?force=true' : ''}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/zip' },
      body: file,
    })
    const body = (await res.json().catch(() => null)) as
      | (PluginInstallResult & { error?: string; message?: string })
      | null
    if (!res.ok) throw new Error(body?.message || body?.error || `install -> ${res.status}`)
    return body as PluginInstallResult
  },
  deletePlugin: async (id: string) => {
    const res = await fetch(`/api/plugins/${id}`, { method: 'DELETE' })
    if (!res.ok) {
      const body = (await res.json().catch(() => null)) as { error?: string; message?: string } | null
      throw new Error(body?.message || body?.error || `DELETE plugin -> ${res.status}`)
    }
  },
  setPluginEnabled: (id: string, enabled: boolean) =>
    sendJson<PluginInfo>(`/api/plugins/${id}/enable`, 'POST', { enabled }),
  pluginSettings: (id: string) => getJson<PluginSettingsResp>(`/api/plugins/${id}/settings`),
  savePluginSettings: async (id: string, values: Record<string, unknown>) => {
    const res = await fetch(`/api/plugins/${id}/settings`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ values }),
    })
    if (!res.ok) throw new Error(`PUT plugin settings -> ${res.status}`)
  },
  // 插件资产 URL（主题 css 等）；?v=版本 破 no-cache 后的旧缓存
  pluginAssetUrl: (id: string, path: string, version: string) =>
    `/api/plugins/${id}/assets/${path}?v=${encodeURIComponent(version)}`,
  // 执行插件 backend 命令（spec §9.4）。失败抛带服务端 message 的 Error。
  runPluginCommand: async (
    pluginId: string,
    commandId: string,
    args: Record<string, unknown>,
  ): Promise<Record<string, unknown>> => {
    const res = await fetch(`/api/plugins/${pluginId}/commands/${commandId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ args }),
    })
    const body = (await res.json().catch(() => null)) as
      | { result?: Record<string, unknown>; error?: string; message?: string }
      | null
    if (!res.ok) throw new Error(body?.message || body?.error || `command -> ${res.status}`)
    return body?.result ?? {}
  },
}

// demo 模式只覆盖只读路径；写入/资源等仍是 httpApi（demo 站点里不会触发）。
const demoApi = {
  status: async (): Promise<StatusResp> => ({
    configured: true,
    source_type: 'demo',
    notes: 0,
    folders: 0,
    read_only: true, // demo 天然只读
    version: '0.0.0-demo',
  }),
  folders: async (): Promise<FolderNode[]> => JSON.parse((await wasmDemo()).folders()),
  notes: async (folderId: string): Promise<NoteSummary[]> =>
    JSON.parse((await wasmDemo()).notes(folderId)),
  note: async (id: string): Promise<NoteDetail> => JSON.parse((await wasmDemo()).note(id)),
  search: async (q: string): Promise<NoteSummary[]> => JSON.parse((await wasmDemo()).search(q)),
}

export const api = DEMO ? { ...httpApi, ...demoApi } : httpApi
