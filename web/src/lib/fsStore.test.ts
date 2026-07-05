// fsStore（FSA 真实文件夹后端）单测：用内存版 fake FileSystemDirectoryHandle 覆盖
// 读/写/删/资源/文件名过滤，无需真实浏览器目录选择器。
import { describe, expect, it } from 'vitest'
import * as fsStore from './fsStore'

// ---- 内存版 fake FSA 句柄 ----
class FakeFile {
	kind = 'file' as const
	constructor(
		public name: string,
		private read: () => string | Blob,
		private commit?: (v: string | Blob) => void,
	) {}
	async getFile(): Promise<Blob | { text: () => Promise<string> }> {
		const c = this.read()
		return c instanceof Blob ? c : { text: async () => c }
	}
	async createWritable() {
		let last: string | Blob = ''
		return {
			write: async (d: string | Blob) => {
				last = d
			},
			close: async () => {
				this.commit?.(last)
			},
		}
	}
}
class FakeDir {
	kind = 'directory' as const
	files = new Map<string, string | Blob>()
	dirs = new Map<string, FakeDir>()
	constructor(public name = '') {}
	async *entries(): AsyncGenerator<[string, FakeFile | FakeDir]> {
		for (const [n] of this.files) yield [n, new FakeFile(n, () => this.files.get(n)!)]
		for (const [n, d] of this.dirs) yield [n, d]
	}
	async getFileHandle(name: string, opts?: { create?: boolean }) {
		if (!this.files.has(name)) {
			if (!opts?.create) throw new Error('NotFoundError')
			this.files.set(name, '')
		}
		return new FakeFile(
			name,
			() => this.files.get(name)!,
			(v) => this.files.set(name, v),
		)
	}
	async getDirectoryHandle(name: string, opts?: { create?: boolean }) {
		if (!this.dirs.has(name)) {
			if (!opts?.create) throw new Error('NotFoundError')
			this.dirs.set(name, new FakeDir(name))
		}
		return this.dirs.get(name)!
	}
	async removeEntry(name: string) {
		if (!this.files.delete(name)) this.dirs.delete(name)
	}
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const asHandle = (d: FakeDir) => d as any

const ID_A = 'a1110000000000000000000000000001'
const ID_B = 'b2220000000000000000000000000002'

describe('fsStore readAllItems 过滤', () => {
	it('只读 <32hex>.md，忽略 info.json / 非 hex / 错扩展名 / 目录', async () => {
		const dir = new FakeDir('lib')
		dir.files.set(`${ID_A}.md`, 'note A raw')
		dir.files.set(`${ID_B}.md`, 'note B raw')
		dir.files.set('info.json', '{}')
		dir.files.set('zz110000000000000000000000000001.md', 'not hex')
		dir.files.set(`${ID_A}.txt`, 'wrong ext')
		dir.dirs.set('.resource', new FakeDir('.resource'))
		const raws = await fsStore.readAllItems(asHandle(dir))
		expect(raws.sort()).toEqual(['note A raw', 'note B raw'])
	})
})

describe('fsStore 条目写/删', () => {
	it('writeItem 建 <id>.md，readAllItems 读到，deleteItemFile 删除', async () => {
		const dir = new FakeDir('lib')
		await fsStore.writeItem(asHandle(dir), ID_A, 'hello raw')
		expect(dir.files.get(`${ID_A}.md`)).toBe('hello raw')
		expect(await fsStore.readAllItems(asHandle(dir))).toEqual(['hello raw'])

		await fsStore.writeItem(asHandle(dir), ID_A, 'updated raw') // 覆盖
		expect(await fsStore.readAllItems(asHandle(dir))).toEqual(['updated raw'])

		await fsStore.deleteItemFile(asHandle(dir), ID_A)
		expect(await fsStore.readAllItems(asHandle(dir))).toEqual([])
		await fsStore.deleteItemFile(asHandle(dir), ID_A) // 幂等：已不存在不抛
	})
})

describe('fsStore 资源二进制（.resource/）', () => {
	it('write/read/list/delete 资源', async () => {
		const dir = new FakeDir('lib')
		const blob = new Blob([new Uint8Array([1, 2, 3])], { type: 'image/png' })
		await fsStore.writeResource(asHandle(dir), ID_A, blob)
		expect(dir.dirs.get('.resource')?.files.get(ID_A)).toBe(blob)
		expect(await fsStore.listResourceIds(asHandle(dir))).toEqual([ID_A])
		expect(await fsStore.readResourceBlob(asHandle(dir), ID_A)).toBe(blob)

		await fsStore.deleteResourceFile(asHandle(dir), ID_A)
		expect(await fsStore.listResourceIds(asHandle(dir))).toEqual([])
		expect(await fsStore.readResourceBlob(asHandle(dir), ID_A)).toBeNull()
	})

	it('无 .resource 目录时 list 空、read 为 null（不抛）', async () => {
		const dir = new FakeDir('lib')
		expect(await fsStore.listResourceIds(asHandle(dir))).toEqual([])
		expect(await fsStore.readResourceBlob(asHandle(dir), ID_A)).toBeNull()
	})
})

describe('fsStore.fsaSupported', () => {
	it('无 showDirectoryPicker → false', () => {
		expect(fsStore.fsaSupported()).toBe(false) // jsdom 无该 API
	})
})
