// 资源元数据缓存（id → {mime,title,…}）：渲染 `:/id` 引用时据此决定用 <img> / <video> /
// <audio> / PDF 卡片 / 下载卡片。启动时全量加载一次，写入类操作即时并入，SSE 资源事件刷新。
//
// 用 rune 存表：render.ts 的 rewriteResourceRefs 在 NoteView 的 $derived(renderNote) 里被调，
// 读 meta[id] 会被该 derived 追踪 → 元数据加载完成后阅读视图自动重渲染。

import { api, type ResourceInfo } from './api'

let meta = $state<Record<string, ResourceInfo>>({})
let inflight: Promise<void> | null = null

// 命令式监听（供编辑器：StateField 非响应式，元数据变化需主动触发 Live Preview 重建）
const listeners = new Set<() => void>()
function notify() {
	for (const l of listeners) l()
}
/** 订阅元数据变化；返回取消订阅。 */
export function onResourceMetaChange(cb: () => void): () => void {
	listeners.add(cb)
	return () => listeners.delete(cb)
}

/** 全量加载资源元数据（幂等：并发调用复用同一请求，除非 force）。失败静默（回落 <img>）。 */
export function loadResourceMeta(force = false): Promise<void> {
	if (inflight && !force) return inflight
	inflight = (async () => {
		try {
			const list = await api.resources()
			const m: Record<string, ResourceInfo> = {}
			for (const r of list) m[r.id] = r
			meta = m
			notify()
		} catch {
			/* 未配置 / 匿名无权限 / 网络 → 保持现状，回落 <img> */
		} finally {
			inflight = null
		}
	})()
	return inflight
}

/** 新上传/重命名后即时并入，避免等下一次全量刷新。 */
export function noteResourceMeta(r: ResourceInfo): void {
	meta = { ...meta, [r.id]: r }
	notify()
}

export type MediaKind = 'image' | 'video' | 'audio' | 'pdf' | 'file' | 'unknown'

/** 资源的媒体类别；'unknown'=元数据尚未加载（渲染方回落 <img>，加载后自动重渲染）。 */
export function mediaKind(id: string): MediaKind {
	const mime = meta[id]?.mime
	if (mime === undefined) return 'unknown'
	if (mime.startsWith('image/')) return 'image'
	if (mime.startsWith('video/')) return 'video'
	if (mime.startsWith('audio/')) return 'audio'
	if (mime === 'application/pdf') return 'pdf'
	return 'file'
}

/** 资源标题（供 PDF/文件卡片显示文件名）。 */
export function resourceTitle(id: string): string {
	return meta[id]?.title ?? ''
}
