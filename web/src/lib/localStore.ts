// IndexedDB 持久层——仅「可写本地应用」构建（WASM_WRITABLE）使用。
//
// 数据落点：
// - kv 存储（out-of-line 键）：'raws'（全部原始 .md 条目数组）/ 'seeded'（是否已播种）/
//   'schema_version'（数据格式版本，供将来迁移）。
// - resources 存储（keyPath=id）：{ id, mime, blob } 资源二进制。
//
// MVP 用「全量快照」持久：每次写入后把 WASM 的 snapshot()（全部 raw）整体写回 'raws'
// （单条记录、单次 put、原子）。个人规模（数百条小字符串）成本可忽略；后续需要再切增量。

const DB_NAME = 'jasper-local'
const DB_VERSION = 1
const KV = 'kv'
const RESOURCES = 'resources'
const SCHEMA_VERSION = 1

export interface StoredResource {
	id: string
	mime: string
	blob: Blob
}

let _db: Promise<IDBDatabase> | null = null

function openDb(): Promise<IDBDatabase> {
	if (_db) return _db
	_db = new Promise((resolve, reject) => {
		const req = indexedDB.open(DB_NAME, DB_VERSION)
		req.onupgradeneeded = () => {
			const db = req.result
			if (!db.objectStoreNames.contains(KV)) db.createObjectStore(KV)
			if (!db.objectStoreNames.contains(RESOURCES)) db.createObjectStore(RESOURCES, { keyPath: 'id' })
		}
		req.onsuccess = () => resolve(req.result)
		req.onerror = () => reject(req.error)
	})
	return _db
}

// 把一次 IDBRequest 包成 Promise。
function reqDone<T>(req: IDBRequest<T>): Promise<T> {
	return new Promise((resolve, reject) => {
		req.onsuccess = () => resolve(req.result)
		req.onerror = () => reject(req.error)
	})
}

async function kvGet<T>(key: string): Promise<T | undefined> {
	const db = await openDb()
	const tx = db.transaction(KV, 'readonly')
	return reqDone<T>(tx.objectStore(KV).get(key) as IDBRequest<T>)
}

async function kvPut(key: string, value: unknown): Promise<void> {
	const db = await openDb()
	const tx = db.transaction(KV, 'readwrite')
	tx.objectStore(KV).put(value, key)
	await txDone(tx)
}

function txDone(tx: IDBTransaction): Promise<void> {
	return new Promise((resolve, reject) => {
		tx.oncomplete = () => resolve()
		tx.onerror = () => reject(tx.error)
		tx.onabort = () => reject(tx.error)
	})
}

/** 读取持久化的原始条目数组；从未持久化过（首次运行）→ null。 */
export async function loadRaws(): Promise<string[] | null> {
	const raws = await kvGet<string[]>('raws')
	return raws ?? null
}

/** 整体写回原始条目数组（写入后的持久化咽喉）。 */
export async function saveRaws(raws: string[]): Promise<void> {
	await kvPut('schema_version', SCHEMA_VERSION)
	await kvPut('raws', raws)
}

/** 是否已用演示库播种过（避免用户清空后又被重新塞满）。 */
export async function isSeeded(): Promise<boolean> {
	return (await kvGet<boolean>('seeded')) === true
}

export async function markSeeded(): Promise<void> {
	await kvPut('seeded', true)
}

/** 存入/覆盖一个资源二进制。 */
export async function putResource(id: string, mime: string, blob: Blob): Promise<void> {
	const db = await openDb()
	const tx = db.transaction(RESOURCES, 'readwrite')
	tx.objectStore(RESOURCES).put({ id, mime, blob } satisfies StoredResource)
	await txDone(tx)
}

export async function getResource(id: string): Promise<StoredResource | undefined> {
	const db = await openDb()
	const tx = db.transaction(RESOURCES, 'readonly')
	return reqDone<StoredResource | undefined>(
		tx.objectStore(RESOURCES).get(id) as IDBRequest<StoredResource | undefined>,
	)
}

export async function deleteResource(id: string): Promise<void> {
	const db = await openDb()
	const tx = db.transaction(RESOURCES, 'readwrite')
	tx.objectStore(RESOURCES).delete(id)
	await txDone(tx)
}

/** 全部资源（含二进制）——用于启动时构建 blob URL 映射。 */
export async function allResources(): Promise<StoredResource[]> {
	const db = await openDb()
	const tx = db.transaction(RESOURCES, 'readonly')
	return reqDone<StoredResource[]>(tx.objectStore(RESOURCES).getAll() as IDBRequest<StoredResource[]>)
}

/** 测试辅助：关闭并丢弃缓存连接（fake-indexeddb 每个用例换新库时用；
 * 必须先关连接，否则后续 deleteDatabase 会被打开的连接阻塞）。 */
export async function _resetForTest(): Promise<void> {
	if (_db) {
		try {
			;(await _db).close()
		} catch {
			/* ignore */
		}
	}
	_db = null
}
