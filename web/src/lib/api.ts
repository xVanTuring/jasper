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
}
