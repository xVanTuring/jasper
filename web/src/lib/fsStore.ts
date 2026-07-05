// FSA（File System Access API）真实文件夹后端——仅 Chromium（'showDirectoryPicker' in window）。
//
// 把用户真实的 Joplin 同步文件夹当数据源：读根目录下的 `<id>.md`，写回同格式（serialize.rs 生成，
// Joplin 下次同步自动拾取），资源二进制在 `.resource/<id>`。等价于 server 的 storage/local.rs，
// 只是 IO 走浏览器 FSA。句柄存 IndexedDB（`jasper-fs`）持久，回访重新授权。
//
// Firefox/Safari 无 showDirectoryPicker（Mozilla 将本地磁盘选择器列为 harmful），故 fsaSupported()
// 为假；降级（webkitdirectory 导入 / zip 导出）另做。

// 标准库尚未收录 FSA 的权限方法与 showDirectoryPicker，最小声明避免 any。
type PermState = 'granted' | 'denied' | 'prompt'
interface FsPermHandle {
	queryPermission(opts: { mode: 'read' | 'readwrite' }): Promise<PermState>
	requestPermission(opts: { mode: 'read' | 'readwrite' }): Promise<PermState>
}
type DirHandle = FileSystemDirectoryHandle & FsPermHandle
declare global {
	interface Window {
		showDirectoryPicker?: (opts?: { mode?: 'read' | 'readwrite' }) => Promise<FileSystemDirectoryHandle>
	}
}

const ITEM_RE = /^[0-9a-fA-F]{32}\.md$/ // 对齐 server::storage::is_item_filename
const RESOURCE_DIR = '.resource'

/** 当前浏览器是否支持选择真实文件夹（Chromium 系）。 */
export function fsaSupported(): boolean {
	return typeof window !== 'undefined' && typeof window.showDirectoryPicker === 'function'
}

// ---------- 句柄持久（IndexedDB jasper-fs / kv / 'dir'）----------
// FileSystemDirectoryHandle 可被 structuredClone，故能直接存进 IndexedDB 跨会话保留。

const HANDLE_DB = 'jasper-fs'
const HANDLE_KEY = 'dir'
let _hdb: Promise<IDBDatabase> | null = null

function handleDb(): Promise<IDBDatabase> {
	if (_hdb) return _hdb
	_hdb = new Promise((resolve, reject) => {
		const req = indexedDB.open(HANDLE_DB, 1)
		req.onupgradeneeded = () => {
			const db = req.result
			if (!db.objectStoreNames.contains('kv')) db.createObjectStore('kv')
		}
		req.onsuccess = () => resolve(req.result)
		req.onerror = () => reject(req.error)
	})
	return _hdb
}

function txDone(tx: IDBTransaction): Promise<void> {
	return new Promise((resolve, reject) => {
		tx.oncomplete = () => resolve()
		tx.onerror = () => reject(tx.error)
		tx.onabort = () => reject(tx.error)
	})
}

export async function saveHandle(handle: FileSystemDirectoryHandle): Promise<void> {
	const db = await handleDb()
	const tx = db.transaction('kv', 'readwrite')
	tx.objectStore('kv').put(handle, HANDLE_KEY)
	await txDone(tx)
}

export async function loadHandle(): Promise<FileSystemDirectoryHandle | null> {
	const db = await handleDb()
	const tx = db.transaction('kv', 'readonly')
	return new Promise((resolve) => {
		const g = tx.objectStore('kv').get(HANDLE_KEY)
		g.onsuccess = () => resolve((g.result as FileSystemDirectoryHandle) ?? null)
		g.onerror = () => resolve(null)
	})
}

export async function clearHandle(): Promise<void> {
	const db = await handleDb()
	const tx = db.transaction('kv', 'readwrite')
	tx.objectStore('kv').delete(HANDLE_KEY)
	await txDone(tx)
}

// ---------- 选择 / 授权 ----------

/** 弹目录选择器（readwrite）。用户取消会 reject（AbortError）。 */
export async function pickDirectory(): Promise<FileSystemDirectoryHandle> {
	if (!window.showDirectoryPicker) throw new Error('showDirectoryPicker unavailable')
	return window.showDirectoryPicker({ mode: 'readwrite' })
}

/** 确认对句柄有读写权限。request=true 时在用户手势里申请（否则只查询）。 */
export async function ensurePermission(
	handle: FileSystemDirectoryHandle,
	request = false,
): Promise<boolean> {
	const h = handle as DirHandle
	const opts = { mode: 'readwrite' } as const
	if ((await h.queryPermission(opts)) === 'granted') return true
	if (request && (await h.requestPermission(opts)) === 'granted') return true
	return false
}

// ---------- 读 ----------

/** 读根目录下全部 Joplin 条目文件 `<id>.md` 的原始文本。 */
export async function readAllItems(handle: FileSystemDirectoryHandle): Promise<string[]> {
	const raws: string[] = []
	for await (const [name, entry] of handle.entries()) {
		if (entry.kind === 'file' && ITEM_RE.test(name)) {
			const file = await (entry as FileSystemFileHandle).getFile()
			raws.push(await file.text())
		}
	}
	return raws
}

/** 读某资源二进制（`.resource/<id>`）；不存在 → null。 */
export async function readResourceBlob(
	handle: FileSystemDirectoryHandle,
	id: string,
): Promise<Blob | null> {
	try {
		const rdir = await handle.getDirectoryHandle(RESOURCE_DIR)
		const fh = await rdir.getFileHandle(id)
		return await fh.getFile()
	} catch {
		return null
	}
}

/** 列出 `.resource/` 下全部资源 id；无该目录 → 空。 */
export async function listResourceIds(handle: FileSystemDirectoryHandle): Promise<string[]> {
	try {
		const rdir = await handle.getDirectoryHandle(RESOURCE_DIR)
		const ids: string[] = []
		for await (const [name, entry] of rdir.entries()) {
			if (entry.kind === 'file') ids.push(name)
		}
		return ids
	} catch {
		return []
	}
}

// ---------- 写 ----------

/** 写回一个条目 `<id>.md`（createWritable 覆盖，close 时提交）。 */
export async function writeItem(
	handle: FileSystemDirectoryHandle,
	id: string,
	raw: string,
): Promise<void> {
	const fh = await handle.getFileHandle(`${id}.md`, { create: true })
	const w = await fh.createWritable()
	await w.write(raw)
	await w.close()
}

/** 删除条目文件 `<id>.md`（不存在视作成功）。 */
export async function deleteItemFile(
	handle: FileSystemDirectoryHandle,
	id: string,
): Promise<void> {
	try {
		await handle.removeEntry(`${id}.md`)
	} catch {
		/* 已不存在 */
	}
}

/** 写资源二进制到 `.resource/<id>`（目录不存在则建）。 */
export async function writeResource(
	handle: FileSystemDirectoryHandle,
	id: string,
	blob: Blob,
): Promise<void> {
	const rdir = await handle.getDirectoryHandle(RESOURCE_DIR, { create: true })
	const fh = await rdir.getFileHandle(id, { create: true })
	const w = await fh.createWritable()
	await w.write(blob)
	await w.close()
}

/** 删除资源二进制 `.resource/<id>`（不存在视作成功）。 */
export async function deleteResourceFile(
	handle: FileSystemDirectoryHandle,
	id: string,
): Promise<void> {
	try {
		const rdir = await handle.getDirectoryHandle(RESOURCE_DIR)
		await rdir.removeEntry(id)
	} catch {
		/* 已不存在 */
	}
}
