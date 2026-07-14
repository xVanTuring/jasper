// 自动保存协调器（纯逻辑，无框架/无 IO）：把「脏标记 + 在途保存 + 快照复核 +
// 外部变更能否安全回显」这套易错的时序判断从 NoteView 抽出来，便于单测。
//
// 根治的 bug：编辑到一半被重置（笔记与光标恢复）。成因是一次自动保存 `await` 期间用户
// 继续打字，保存返回后无条件清 dirty，把含更新输入的缓冲误判为「已保存」；随后自己写入
// 触发的 SSE 回声经 applyExternal 用服务端旧值 setValue 整篇替换缓冲 → 输入与光标被重置。
// 修复要点：保存前对发出的内容做快照，返回后仅当缓冲仍等于该快照才转干净，否则保持脏并再存；
// 且「有未保存输入或正在保存」时一律拒绝外部回显。

export interface NoteSnapshot {
	title: string
	body: string
}

function sameSnapshot(a: NoteSnapshot, b: NoteSnapshot): boolean {
	return a.title === b.title && a.body === b.body
}

export class Autosave {
	// 缓冲相对「上一次成功保存到服务端的内容」是否有未保存改动
	dirty = false
	// 是否有一次 updateNote 请求在途（避免并发写，也用于回显门控）
	saving = false

	// 用户编辑：置脏。程序化写入（外部回显）不应调用本方法。
	markDirty(): void {
		this.dirty = true
	}

	// 是否应发起一次保存：有脏且当前无在途请求。
	canBeginSave(): boolean {
		return this.dirty && !this.saving
	}

	// 标记保存开始（进入在途）。调用方应在此刻对要发出的内容取快照。
	beginSave(): void {
		this.saving = true
	}

	// 保存成功返回。cur=返回时刻的缓冲当前值，saved=本次实际发出的快照。
	// 缓冲未变 → 转干净，返回 true；保存期间又改动 → 仍脏，返回 false（调用方需再排一次保存）。
	finishSaveOk(cur: NoteSnapshot, saved: NoteSnapshot): boolean {
		this.saving = false
		if (sameSnapshot(cur, saved)) {
			this.dirty = false
			return true
		}
		return false
	}

	// 保存失败：退出在途，dirty 保持不变（内容仍未落盘）。
	finishSaveErr(): void {
		this.saving = false
	}

	// 外部变更（SSE / 别的客户端 / 直接写文件）能否安全回显到编辑器缓冲：
	// 有未保存输入或正在保存 → 拒绝（绝不打断/覆盖用户输入）；内容与缓冲相同 → 无需回显。
	canApplyExternal(cur: NoteSnapshot, fresh: NoteSnapshot): boolean {
		if (this.dirty || this.saving) return false
		return !sameSnapshot(cur, fresh)
	}
}
