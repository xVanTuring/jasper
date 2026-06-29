// 后端 API 客户端。开发期经 Vite 代理到 27583，生产期同源访问。

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

export const api = {
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
    if (!res.ok) throw new Error(`上传失败 -> ${res.status}`)
    return res.json() as Promise<ResourceUpload>
  },

  // 资源管理
  resources: () => getJson<ResourceInfo[]>('/api/resources'),
  renameResource: (id: string, title: string) =>
    sendJson<ResourceInfo>(`/api/resources/${id}`, 'PUT', { title }),
  deleteResource: async (id: string) => {
    const res = await fetch(`/api/resources/${id}`, { method: 'DELETE' })
    if (!res.ok) throw new Error(`删除资源 -> ${res.status}`)
  },
}
