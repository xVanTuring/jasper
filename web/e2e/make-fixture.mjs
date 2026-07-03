// 生成一个最小的 Joplin 格式测试库（供 e2e 全栈测试的本地数据源用）。
// 字段集/顺序对齐 core/src/serialize.rs（new_note_md / new_folder_md / new_resource_md）。
// 用固定时间戳，保证可复现。
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

// 固定 32hex id（十六进制合法即可）
export const IDS = {
	notebook: '11111111111111111111111111111111',
	imageNote: '22222222222222222222222222222222',
	todoNote: '33333333333333333333333333333333',
	plainNote: '44444444444444444444444444444444',
	resource: '55555555555555555555555555555555',
	// 标签测试：一篇预打标签的笔记 + 一个标签 + 一条关联
	tagNote: '66666666666666666666666666666666',
	tag: '77777777777777777777777777777777',
	noteTag: '88888888888888888888888888888888',
}

const TS = 1700000000000 // 固定时间
const ISO = new Date(TS).toISOString() // YYYY-MM-DDTHH:mm:ss.SSSZ

// 1x1 透明 PNG
const PNG_BASE64 =
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII='

function noteMd(id, parentId, title, body, isTodo) {
	const props = [
		`id: ${id}`,
		`parent_id: ${parentId}`,
		`created_time: ${ISO}`,
		`updated_time: ${ISO}`,
		'is_conflict: 0',
		'latitude: 0.00000000',
		'longitude: 0.00000000',
		'altitude: 0.0000',
		'author: ',
		'source_url: ',
		`is_todo: ${isTodo ? 1 : 0}`,
		'todo_due: 0',
		'todo_completed: 0',
		'source: jasper',
		'source_application: net.cozic.jasper',
		'application_data: ',
		'order: 0',
		`user_created_time: ${ISO}`,
		`user_updated_time: ${ISO}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		'markup_language: 1',
		'is_shared: 0',
		'share_id: ',
		'conflict_original_id: ',
		'master_key_id: ',
		'user_data: ',
		'deleted_time: 0',
		'type_: 1',
	]
	return `${title}\n\n${body}\n\n${props.join('\n')}`
}

function folderMd(id, parentId, title) {
	const props = [
		`id: ${id}`,
		`created_time: ${ISO}`,
		`updated_time: ${ISO}`,
		`user_created_time: ${ISO}`,
		`user_updated_time: ${ISO}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		`parent_id: ${parentId}`,
		'is_shared: 0',
		'share_id: ',
		'master_key_id: ',
		'icon: ',
		'user_data: ',
		'deleted_time: 0',
		'type_: 2',
	]
	return `${title}\n\n${props.join('\n')}`
}

function resourceMd(id, title, mime, ext, size) {
	const props = [
		`id: ${id}`,
		`mime: ${mime}`,
		'filename: ',
		`created_time: ${ISO}`,
		`updated_time: ${ISO}`,
		`user_created_time: ${ISO}`,
		`user_updated_time: ${ISO}`,
		`file_extension: ${ext}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		'encryption_blob_encrypted: 0',
		`size: ${size}`,
		'is_shared: 0',
		'share_id: ',
		'master_key_id: ',
		'user_data: ',
		`blob_updated_time: ${TS}`,
		'ocr_text: ',
		'ocr_details: ',
		'ocr_status: 0',
		'ocr_error: ',
		'ocr_driver_id: 1',
		'type_: 4',
	]
	return `${title}\n\n${props.join('\n')}`
}

// 标签条目（type_=5）：字段集/顺序对齐 core/src/serialize.rs::new_tag_md（= Joplin 真实数据）。
function tagMd(id, title) {
	const props = [
		`id: ${id}`,
		`created_time: ${ISO}`,
		`updated_time: ${ISO}`,
		`user_created_time: ${ISO}`,
		`user_updated_time: ${ISO}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		'is_shared: 0',
		'parent_id: ',
		'user_data: ',
		'type_: 5',
	]
	return `${title}\n\n${props.join('\n')}`
}

// note_tag 关联（type_=6，纯元数据无标题）：对齐 core/src/serialize.rs::new_note_tag_md。
function noteTagMd(id, noteId, tagId) {
	const props = [
		`id: ${id}`,
		`note_id: ${noteId}`,
		`tag_id: ${tagId}`,
		`created_time: ${ISO}`,
		`updated_time: ${ISO}`,
		`user_created_time: ${ISO}`,
		`user_updated_time: ${ISO}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		'is_shared: 0',
		'type_: 6',
	]
	return props.join('\n')
}

/** 在 `dir` 下写出一个完整可读的 Joplin 库，返回 IDS。 */
export function makeFixture(dir) {
	mkdirSync(join(dir, '.resource'), { recursive: true })

	writeFileSync(
		join(dir, 'info.json'),
		'{"version":3,"e2ee":{"value":false,"updatedTime":0},"activeMasterKeyId":{"value":"","updatedTime":0},"masterKeys":[],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}',
	)

	writeFileSync(join(dir, `${IDS.notebook}.md`), folderMd(IDS.notebook, '', 'Notebook'))

	// 图片笔记：正文含 ![说明](:/资源)，用于 alt 语义往返测试
	writeFileSync(
		join(dir, `${IDS.imageNote}.md`),
		noteMd(
			IDS.imageNote,
			IDS.notebook,
			'Image Note',
			`Intro line.\n\n![说明](:/${IDS.resource})`,
			false,
		),
	)

	writeFileSync(
		join(dir, `${IDS.todoNote}.md`),
		noteMd(IDS.todoNote, IDS.notebook, 'Todo Note', '- [ ] task one\n- [x] task two', true),
	)

	writeFileSync(
		join(dir, `${IDS.plainNote}.md`),
		noteMd(IDS.plainNote, IDS.notebook, 'Plain Note', 'Just some **markdown** text.', false),
	)

	const png = Buffer.from(PNG_BASE64, 'base64')
	writeFileSync(join(dir, '.resource', IDS.resource), png)
	writeFileSync(
		join(dir, `${IDS.resource}.md`),
		resourceMd(IDS.resource, 'pixel.png', 'image/png', 'png', png.length),
	)

	// 预打标签的笔记：'Tagged Note' 打了 'reading' 标签（供标签浏览/打标签 e2e）
	writeFileSync(
		join(dir, `${IDS.tagNote}.md`),
		noteMd(IDS.tagNote, IDS.notebook, 'Tagged Note', 'A note that carries a tag.', false),
	)
	writeFileSync(join(dir, `${IDS.tag}.md`), tagMd(IDS.tag, 'reading'))
	writeFileSync(join(dir, `${IDS.noteTag}.md`), noteTagMd(IDS.noteTag, IDS.tagNote, IDS.tag))

	return IDS
}

// 允许直接 `node make-fixture.mjs <dir>` 手动生成，便于调试
if (import.meta.url === `file://${process.argv[1]}`) {
	const dir = process.argv[2]
	if (!dir) {
		console.error('usage: node make-fixture.mjs <dir>')
		process.exit(1)
	}
	makeFixture(dir)
	console.log('fixture written to', dir)
}
