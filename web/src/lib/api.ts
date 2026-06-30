// 后端 API 客户端。开发期经 Vite 代理到 27583，生产期同源访问。
import { t } from './i18n.svelte'

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
  source_type: string // 'local' | 'webdav'
  local_path: string
  webdav_url: string
  webdav_user: string
  webdav_pass: string
}

export interface StatusResp {
  configured: boolean
  source_type: string
  notes: number
  folders: number
}

export interface ConfigResult {
  ok: boolean
  error: string | null
  notes: number
  folders: number
}

export interface ApplyConfigReq extends SourceConfig {
  create_new: boolean
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

const httpApi = {
  status: () => getJson<StatusResp>('/api/status'),
  getConfig: () => getJson<SourceConfig>('/api/config'),
  saveConfig: (data: ApplyConfigReq) => sendJson<ConfigResult>('/api/config', 'PUT', data),

  folders: () => getJson<FolderNode[]>('/api/folders'),
  notes: (folderId: string) =>
    getJson<NoteSummary[]>(`/api/notes?folder=${encodeURIComponent(folderId)}`),
  note: (id: string) => getJson<NoteDetail>(`/api/notes/${id}`),
  search: (q: string) => getJson<NoteSummary[]>(`/api/search?q=${encodeURIComponent(q)}`),
  resourceUrl: (id: string) => `/api/resources/${id}`,

  // 写入
  updateNote: (id: string, data: { title: string; body: string }) =>
    sendJson<NoteDetail>(`/api/notes/${id}`, 'PUT', data),
  createNote: (data: { parent_id: string; title?: string; body?: string }) =>
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
}

// demo 模式只覆盖只读路径；写入/资源等仍是 httpApi（demo 站点里不会触发）。
const demoApi = {
  status: async (): Promise<StatusResp> => ({
    configured: true,
    source_type: 'demo',
    notes: 0,
    folders: 0,
  }),
  folders: async (): Promise<FolderNode[]> => JSON.parse((await wasmDemo()).folders()),
  notes: async (folderId: string): Promise<NoteSummary[]> =>
    JSON.parse((await wasmDemo()).notes(folderId)),
  note: async (id: string): Promise<NoteDetail> => JSON.parse((await wasmDemo()).note(id)),
  search: async (q: string): Promise<NoteSummary[]> => JSON.parse((await wasmDemo()).search(q)),
}

export const api = DEMO ? { ...httpApi, ...demoApi } : httpApi
