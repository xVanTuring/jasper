// 生成一个「演示 / 截图」用的 Joplin 格式库，支持中/英两套内容（结构对齐、便于并排出图）。
// 字段集对齐 core/src/serialize.rs 与 docs/gen-demo-library.py；时间戳固定，保证可复现。
// 供 web/scripts/shoot.mjs 分别以 en/zh 生成库 → 起后端 → Playwright 截图。
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

// 固定 32hex id：两种语言复用同一套 id，笔记本/笔记一一对应。
const F_TECH = 'a1110000000000000000000000000001'
const F_WORK = 'a1110000000000000000000000000002'
const F_PERS = 'a1110000000000000000000000000003'
const RES_ID = 'b2220000000000000000000000000001'
const N_SHOWCASE = 'c1110000000000000000000000000001'
const N_RUST = 'c1110000000000000000000000000002'
const N_PLAN = 'c1110000000000000000000000000003'
const N_MEETING = 'c1110000000000000000000000000004'
const N_READING = 'c1110000000000000000000000000005'
const N_TRAVEL = 'c1110000000000000000000000000006'

function folderMd(id, title, parent, t) {
	const props = [
		`id: ${id}`,
		`created_time: ${t}`,
		`updated_time: ${t}`,
		`user_created_time: ${t}`,
		`user_updated_time: ${t}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		`parent_id: ${parent}`,
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

function noteMd(id, parent, title, body, t) {
	const props = [
		`id: ${id}`,
		`parent_id: ${parent}`,
		`created_time: ${t}`,
		`updated_time: ${t}`,
		'is_conflict: 0',
		'latitude: 0.00000000',
		'longitude: 0.00000000',
		'altitude: 0.0000',
		'author: ',
		'source_url: ',
		'is_todo: 0',
		'todo_due: 0',
		'todo_completed: 0',
		'source: jasper',
		'source_application: net.cozic.jasper',
		'application_data: ',
		'order: 0',
		`user_created_time: ${t}`,
		`user_updated_time: ${t}`,
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

function resourceMd(id, title, mime, ext, size, t) {
	const props = [
		`id: ${id}`,
		`mime: ${mime}`,
		'filename: ',
		`created_time: ${t}`,
		`updated_time: ${t}`,
		`user_created_time: ${t}`,
		`user_updated_time: ${t}`,
		`file_extension: ${ext}`,
		'encryption_cipher_text: ',
		'encryption_applied: 0',
		'encryption_blob_encrypted: 0',
		`size: ${size}`,
		'is_shared: 0',
		'share_id: ',
		'master_key_id: ',
		'user_data: ',
		'blob_updated_time: 1718000000000',
		'ocr_text: ',
		'ocr_details: ',
		'ocr_status: 0',
		'ocr_error: ',
		'ocr_driver_id: 1',
		'type_: 4',
	]
	return `${title}\n\n${props.join('\n')}`
}

// 架构图 SVG（三方框：浏览器 SPA → Rust 服务 → 存储），文字随语言切换。
function architectureSvg(lang) {
	const L =
		lang === 'zh'
			? {
					title: 'Jasper · 架构',
					subtitle: '本地 Rust 服务 + 浏览器 SPA，无 Electron',
					spa: '浏览器 SPA',
					spaSub: 'Svelte 5 · CodeMirror',
					srv: 'Rust 服务',
					srvSub: 'axum · ~10MB 常驻',
					store: '存储',
					storeA: '本地文件夹',
					storeB: '/ WebDAV',
					http: 'HTTP',
					rw: '读写',
				}
			: {
					title: 'Jasper · Architecture',
					subtitle: 'Local Rust server + browser SPA, no Electron',
					spa: 'Browser SPA',
					spaSub: 'Svelte 5 · CodeMirror',
					srv: 'Rust server',
					srvSub: 'axum · ~10MB resident',
					store: 'Storage',
					storeA: 'Local folder',
					storeB: '/ WebDAV',
					http: 'HTTP',
					rw: 'read/write',
				}
	return `<svg xmlns="http://www.w3.org/2000/svg" width="700" height="300" viewBox="0 0 700 300" font-family="-apple-system,Segoe UI,Roboto,sans-serif">
  <defs>
    <linearGradient id="bar" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#7c4dff"/><stop offset="1" stop-color="#4d8bff"/>
    </linearGradient>
    <marker id="a" markerWidth="9" markerHeight="9" refX="7" refY="3" orient="auto"><path d="M0,0 L7,3 L0,6 Z" fill="#6b7080"/></marker>
  </defs>
  <rect width="700" height="300" rx="18" fill="#0f1117"/>
  <rect x="0" y="0" width="700" height="6" fill="url(#bar)"/>
  <text x="36" y="56" fill="#e8eaf0" font-size="24" font-weight="700">${L.title}</text>
  <text x="36" y="84" fill="#8b90a0" font-size="14">${L.subtitle}</text>
  <g>
    <rect x="36"  y="120" width="180" height="110" rx="12" fill="#1a1d28" stroke="#7c4dff" stroke-width="1.5"/>
    <text x="126" y="166" fill="#fff" font-size="17" font-weight="600" text-anchor="middle">${L.spa}</text>
    <text x="126" y="190" fill="#9aa0b4" font-size="13" text-anchor="middle">${L.spaSub}</text>
  </g>
  <g>
    <rect x="260" y="120" width="180" height="110" rx="12" fill="#1a1d28" stroke="#4d8bff" stroke-width="1.5"/>
    <text x="350" y="166" fill="#fff" font-size="17" font-weight="600" text-anchor="middle">${L.srv}</text>
    <text x="350" y="190" fill="#9aa0b4" font-size="13" text-anchor="middle">${L.srvSub}</text>
  </g>
  <g>
    <rect x="484" y="120" width="180" height="110" rx="12" fill="#1a1d28" stroke="#39c0a0" stroke-width="1.5"/>
    <text x="574" y="160" fill="#fff" font-size="17" font-weight="600" text-anchor="middle">${L.store}</text>
    <text x="574" y="184" fill="#9aa0b4" font-size="13" text-anchor="middle">${L.storeA}</text>
    <text x="574" y="204" fill="#9aa0b4" font-size="13" text-anchor="middle">${L.storeB}</text>
  </g>
  <path d="M216 175 L260 175" stroke="#6b7080" stroke-width="2" marker-end="url(#a)"/>
  <path d="M440 175 L484 175" stroke="#6b7080" stroke-width="2" marker-end="url(#a)"/>
  <text x="238" y="166" fill="#6b7080" font-size="11" text-anchor="middle">${L.http}</text>
  <text x="462" y="166" fill="#6b7080" font-size="11" text-anchor="middle">${L.rw}</text>
</svg>`
}

// 两套内容。结构一致，仅文案不同；EN 全英文、ZH 全中文。
const CONTENT = {
	en: {
		resTitle: 'architecture.svg',
		folders: { tech: 'Tech Notes', work: 'Work', pers: 'Personal' },
		showcaseTitle: '✨ Feature Showcase',
		showcase: (res) => `Jasper is a **lightweight, read-write** Joplin-compatible client. Here's a quick tour of what it renders.

![Architecture](:/${res})

## What it does
- Reads & writes a Joplin sync library on a **local folder** or **WebDAV**
- Live Markdown rendering: code highlighting, tables, math, task lists
- Resources / images: paste, drag-and-drop, attach — and manage them in a panel

> The backend stays resident at ~10 MB — fast startup, cross-platform, no Electron.

### Code highlighting
\`\`\`rust
fn main() {
    let greeting = "Hello, Jasper";
    println!("{greeting}");
}
\`\`\`

### Tables
| Data source | Read | Write |
|-------------|------|-------|
| Local folder | ✅ | ✅ |
| WebDAV | ✅ | ✅ |

### Task list
- [x] Local / WebDAV reading
- [x] Incremental cache
- [x] Resource upload & management
- [ ] Tag view

### Math
Inline $E = mc^2$, and a display equation:

$$\\int_0^\\infty e^{-x}\\,dx = 1$$
`,
		rustTitle: 'Rust Ownership & Borrowing',
		rust: `Three rules of ownership:

1. Every value has exactly one **owner**
2. There can only be one owner at a time
3. When the owner goes out of scope, the value is dropped

\`\`\`rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
\`\`\`

The borrow checker guarantees memory safety **at compile time** — no GC.
`,
		planTitle: 'Q3 Project Plan',
		plan: `## Goals
- Finish incremental cache and resource management
- End-to-end WebDAV testing

| Milestone | Status |
|-----------|--------|
| Incremental cache | ✅ Done |
| Resource upload | ✅ Done |
| Auth | 🚧 In progress |
`,
		meetingTitle: 'Weekly Sync 2026-06-29',
		meeting: `**Time**: 2026-06-29 10:00
**Attendees**: everyone

### Decisions
- Incremental cache shipped; on WebDAV, startup only fetches changed items
- Resource panel supports one-click orphan cleanup

### Action items
- [ ] Flesh out the README
- [ ] Prepare demo screenshots
`,
		readingTitle: '📚 Reading List',
		reading: `- [x] "The Rust Programming Language"
- [x] "Computer Systems: A Programmer's Perspective"
- [ ] "Designing Data-Intensive Applications"

> Write a one-page note after finishing each.
`,
		travelTitle: '🏔 Trip Notes',
		travel: (res) => `Spent the weekend in the mountains — poor signal, great views.

![Architecture](:/${res})

- Brought a paper book
- Took lots of photos
- Synced my notes back to Joplin when I got home
`,
	},
	zh: {
		resTitle: '架构图.svg',
		folders: { tech: '技术笔记 · Tech', work: '工作 · Work', pers: '个人 · Personal' },
		showcaseTitle: '✨ 功能展示 Feature Showcase',
		showcase: (res) => `Jasper 是一个**轻量、可读可写**的 Joplin 兼容客户端。下面展示渲染能力。

![架构图](:/${res})

## 它能做什么
- 直接读写本地文件夹或 **WebDAV** 上的 Joplin 同步库
- Markdown 实时渲染：代码高亮、表格、数学公式、任务清单
- 资源/图片：粘贴、拖拽、附件上传，并可在面板里管理

> 后端常驻内存约 10MB，启动快、跨平台、不依赖 Electron。

### 代码高亮
\`\`\`rust
fn main() {
    let greeting = "你好, Jasper";
    println!("{greeting}");
}
\`\`\`

### 表格
| 数据源 | 读 | 写 |
|--------|----|----|
| 本地文件夹 | ✅ | ✅ |
| WebDAV | ✅ | ✅ |

### 任务清单
- [x] 本地 / WebDAV 读取
- [x] 增量缓存
- [x] 资源上传与管理
- [ ] 标签视图

### 数学公式
行内 $E = mc^2$，独立公式：

$$\\int_0^\\infty e^{-x}\\,dx = 1$$
`,
		rustTitle: 'Rust 所有权与借用',
		rust: `所有权三条规则：

1. 每个值有且仅有一个**所有者**
2. 同一时刻只能有一个所有者
3. 所有者离开作用域，值被丢弃

\`\`\`rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
\`\`\`

借用检查器在**编译期**保证内存安全，无需 GC。
`,
		planTitle: 'Q3 项目计划',
		plan: `## 目标
- 完成增量缓存与资源管理
- WebDAV 端到端联调

| 里程碑 | 状态 |
|--------|------|
| 增量缓存 | ✅ 已完成 |
| 资源上传 | ✅ 已完成 |
| 鉴权 | 🚧 进行中 |
`,
		meetingTitle: '周会纪要 2026-06-29',
		meeting: `**时间**：2026-06-29 10:00
**参会**：全体

### 结论
- 增量缓存上线，WebDAV 启动只拉变化项
- 资源面板支持孤儿清理

### 待办
- [ ] 补充 README
- [ ] 准备演示截图
`,
		readingTitle: '📚 读书清单',
		reading: `- [x] 《Rust 程序设计语言》
- [x] 《深入理解计算机系统》
- [ ] 《数据密集型应用系统设计》
- [ ] 《Designing Data-Intensive Applications》

> 读完做一页纸笔记。
`,
		travelTitle: '🏔 旅行碎记',
		travel: (res) => `周末去了山里，信号很差但风景好。

![架构图](:/${res})

- 带了纸质书
- 拍了很多照片
- 回来把笔记同步回 Joplin
`,
	},
}

// 固定时间戳（ISO UTC），保证列表日期/排序稳定可复现。
const T = {
	tech: '2026-06-10T09:00:00.000Z',
	work: '2026-06-11T09:00:00.000Z',
	pers: '2026-06-12T09:00:00.000Z',
	res: '2026-06-20T08:00:00.000Z',
	showcase: '2026-06-28T15:20:00.000Z',
	rust: '2026-06-26T11:00:00.000Z',
	plan: '2026-06-29T09:30:00.000Z',
	meeting: '2026-06-29T10:40:00.000Z',
	reading: '2026-06-22T20:00:00.000Z',
	travel: '2026-06-15T18:00:00.000Z',
}

/** 在 `dir` 下写出一个演示库（lang='en'|'zh'）。返回关键 id/标题，供截图脚本定位。 */
export function makeDemoLibrary(dir, lang) {
	const c = CONTENT[lang]
	if (!c) throw new Error(`unknown lang: ${lang}`)
	mkdirSync(join(dir, '.resource'), { recursive: true })

	writeFileSync(
		join(dir, 'info.json'),
		'{"version":3,"e2ee":{"value":false,"updatedTime":0},"activeMasterKeyId":{"value":"","updatedTime":0},"masterKeys":[],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}',
	)

	// 资源：架构图 SVG（二进制 + type_=4 元数据）
	const svg = Buffer.from(architectureSvg(lang), 'utf-8')
	writeFileSync(join(dir, '.resource', RES_ID), svg)
	writeFileSync(
		join(dir, `${RES_ID}.md`),
		resourceMd(RES_ID, c.resTitle, 'image/svg+xml', 'svg', svg.length, T.res),
	)

	// 笔记本
	writeFileSync(join(dir, `${F_TECH}.md`), folderMd(F_TECH, c.folders.tech, '', T.tech))
	writeFileSync(join(dir, `${F_WORK}.md`), folderMd(F_WORK, c.folders.work, '', T.work))
	writeFileSync(join(dir, `${F_PERS}.md`), folderMd(F_PERS, c.folders.pers, '', T.pers))

	// 笔记
	writeFileSync(
		join(dir, `${N_SHOWCASE}.md`),
		noteMd(N_SHOWCASE, F_TECH, c.showcaseTitle, c.showcase(RES_ID), T.showcase),
	)
	writeFileSync(join(dir, `${N_RUST}.md`), noteMd(N_RUST, F_TECH, c.rustTitle, c.rust, T.rust))
	writeFileSync(join(dir, `${N_PLAN}.md`), noteMd(N_PLAN, F_WORK, c.planTitle, c.plan, T.plan))
	writeFileSync(
		join(dir, `${N_MEETING}.md`),
		noteMd(N_MEETING, F_WORK, c.meetingTitle, c.meeting, T.meeting),
	)
	writeFileSync(
		join(dir, `${N_READING}.md`),
		noteMd(N_READING, F_PERS, c.readingTitle, c.reading, T.reading),
	)
	writeFileSync(
		join(dir, `${N_TRAVEL}.md`),
		noteMd(N_TRAVEL, F_PERS, c.travelTitle, c.travel(RES_ID), T.travel),
	)

	return {
		folders: c.folders,
		titles: {
			showcase: c.showcaseTitle,
			rust: c.rustTitle,
			plan: c.planTitle,
			meeting: c.meetingTitle,
			reading: c.readingTitle,
			travel: c.travelTitle,
		},
		resTitle: c.resTitle,
	}
}
