// 纯字符串 markdown 工具：格式化 + 块级前缀切换。
// 无 DOM / 无编辑器依赖 → 可单测、可被源码/富文本任一适配器复用，将来也可给插件调用。

// ---------- 块级前缀切换（标题/引用/列表/待办） ----------

export type BlockKind = 'h1' | 'h2' | 'quote' | 'bullet' | 'ordered' | 'task'

const HEADING_RE = /^(#{1,6})\s+/
const QUOTE_RE = /^>\s+/
// 一个列表标记：无序 `-|*|+`、有序 `1.`/`1)`、可选紧跟 ` [ ]`/`[x]` 复选框（待办）
const LIST_RE = /^(\d+[.)]|[-*+])(\s+\[[ xX]\])?\s+/

type Family = 'heading' | 'quote' | 'list'

function familyOf(kind: BlockKind): Family {
	if (kind === 'h1' || kind === 'h2') return 'heading'
	if (kind === 'quote') return 'quote'
	return 'list'
}

// 解析一行在指定 family 下的既有标记类型与去掉标记后的正文。
function parseLine(line: string, family: Family): { type: BlockKind | null; rest: string } {
	if (family === 'heading') {
		const m = line.match(HEADING_RE)
		if (!m) return { type: null, rest: line }
		const level = m[1].length
		return { type: level === 1 ? 'h1' : level === 2 ? 'h2' : null, rest: line.slice(m[0].length) }
	}
	if (family === 'quote') {
		const m = line.match(QUOTE_RE)
		return m ? { type: 'quote', rest: line.slice(m[0].length) } : { type: null, rest: line }
	}
	const m = line.match(LIST_RE)
	if (!m) return { type: null, rest: line }
	const type: BlockKind = m[2] ? 'task' : /^\d+[.)]$/.test(m[1]) ? 'ordered' : 'bullet'
	return { type, rest: line.slice(m[0].length) }
}

function marker(kind: BlockKind, ordinal: number): string {
	switch (kind) {
		case 'h1': return '# '
		case 'h2': return '## '
		case 'quote': return '> '
		case 'bullet': return '- '
		case 'task': return '- [ ] '
		case 'ordered': return `${ordinal}. `
	}
}

// 对给定若干行切换某块级前缀：
//  - 若所有行已是该类型 → 去掉前缀（关闭）
//  - 否则 → 统一设为该类型（替换同族既有标记，有序列表重新编号）
// 返回变换后的行数组（长度不变）。纯函数，便于单测。
export function toggleBlockLines(lines: string[], kind: BlockKind): string[] {
	const family = familyOf(kind)
	const parsed = lines.map((l) => parseLine(l, family))
	const allTarget = parsed.length > 0 && parsed.every((p) => p.type === kind)
	if (allTarget) return parsed.map((p) => p.rest)
	let ordinal = 1
	return parsed.map((p) => marker(kind, ordinal++) + p.rest)
}

// ---------- 格式化（零依赖规范化） ----------

// 统计可视宽度：CJK / 全角计 2，其余计 1（让中文表格在等宽字体下真正对齐）。
function visualWidth(s: string): number {
	let w = 0
	for (const ch of s) {
		const c = ch.codePointAt(0) as number
		const wide =
			(c >= 0x1100 && c <= 0x115f) || // Hangul Jamo
			(c >= 0x2e80 && c <= 0x303e) || // CJK 部首 / 标点
			(c >= 0x3041 && c <= 0x33ff) || // 假名 / CJK 符号
			(c >= 0x3400 && c <= 0x4dbf) || // CJK 扩展 A
			(c >= 0x4e00 && c <= 0x9fff) || // CJK 统一
			(c >= 0xa000 && c <= 0xa4cf) ||
			(c >= 0xac00 && c <= 0xd7a3) || // Hangul 音节
			(c >= 0xf900 && c <= 0xfaff) || // CJK 兼容
			(c >= 0xfe30 && c <= 0xfe4f) || // CJK 兼容形式
			(c >= 0xff00 && c <= 0xff60) || // 全角 ASCII
			(c >= 0xffe0 && c <= 0xffe6) ||
			(c >= 0x20000 && c <= 0x3fffd) // CJK 扩展 B+
		w += wide ? 2 : 1
	}
	return w
}

type Align = 'left' | 'center' | 'right'

function pad(s: string, width: number, align: Align): string {
	const gap = Math.max(0, width - visualWidth(s))
	if (align === 'right') return ' '.repeat(gap) + s
	if (align === 'center') {
		const l = gap >> 1
		return ' '.repeat(l) + s + ' '.repeat(gap - l)
	}
	return s + ' '.repeat(gap)
}

// 拆分表格一行为单元格（按未转义的 `|` 切；尊重 `\|`），去掉首尾边框与空格。
function splitRow(line: string): string[] {
	return line
		.replace(/^\s*\|/, '')
		.replace(/\|\s*$/, '')
		.split(/(?<!\\)\|/)
		.map((c) => c.trim())
}

const DELIM_CELL = /^:?-+:?$/

// 是否为对齐分隔行（表格第二行），如 `|:---|---:|`
function isDelimiterRow(line: string): boolean {
	const cells = splitRow(line)
	return cells.length > 0 && cells.every((c) => DELIM_CELL.test(c))
}

function alignOf(cell: string): Align {
	const l = cell.startsWith(':')
	const r = cell.endsWith(':')
	if (l && r) return 'center'
	if (r) return 'right'
	return 'left'
}

// 把一段 GFM 表格（表头 + 分隔行 + 若干正文行）重排为列对齐。
function formatTable(rows: string[]): string[] {
	const header = splitRow(rows[0])
	const aligns = splitRow(rows[1]).map(alignOf)
	const body = rows.slice(2).map(splitRow)
	const cols = Math.max(header.length, aligns.length, ...body.map((r) => r.length))
	const at = (row: string[], i: number) => row[i] ?? ''
	const widths: number[] = []
	for (let i = 0; i < cols; i++) {
		let w = visualWidth(at(header, i))
		for (const r of body) w = Math.max(w, visualWidth(at(r, i)))
		// 分隔行下限：左 `-`(1)、右 `-:`(2)、居中 `:-:`(3)
		const a = aligns[i] ?? 'left'
		widths[i] = Math.max(a === 'center' ? 3 : a === 'right' ? 2 : 1, w)
	}
	const row = (r: string[]) =>
		'| ' + widths.map((w, i) => pad(at(r, i), w, aligns[i] ?? 'left')).join(' | ') + ' |'
	const delim =
		'| ' +
		widths
			.map((w, i) => {
				const a = aligns[i] ?? 'left'
				const dash = '-'.repeat(Math.max(1, w - (a === 'center' ? 2 : a === 'right' ? 1 : 0)))
				return a === 'center' ? `:${dash}:` : a === 'right' ? `${dash}:` : dash
			})
			.join(' | ') +
		' |'
	return [row(header), delim, ...body.map(row)]
}

// 规范化非表格、非代码块的普通行。
function normalizeLine(line: string): string {
	if (/^\s*$/.test(line)) return '' // 纯空白行 → 空行（保留内容行尾空格，避免毁掉硬换行）
	// 标题：`#` 后恰好一个空格
	let m = line.match(/^(\s*)(#{1,6})\s+(.*)$/)
	if (m) return `${m[1]}${m[2]} ${m[3]}`
	// 无序列表：`*`/`+` → `-`，标记后单空格
	m = line.match(/^(\s*)[-*+]\s+(.*)$/)
	if (m) return `${m[1]}- ${m[2]}`
	// 有序列表：保留序号，`.`/`)` 后单空格
	m = line.match(/^(\s*)(\d+)([.)])\s+(.*)$/)
	if (m) return `${m[1]}${m[2]}${m[3]} ${m[4]}`
	return line
}

// 零依赖 markdown 规范化：表格列对齐、标题/列表标记规整、空白行折叠、结尾单换行。
// 代码块（``` / ~~~ 围栏）内原样保留。保守：不动内容行尾空格（可能是硬换行）。
export function formatMarkdown(src: string): string {
	const lines = src.replace(/\r\n?/g, '\n').split('\n')
	// v=true 表示围栏代码块内容（逐字保留，不参与空行折叠）
	const out: { t: string; v: boolean }[] = []
	let inFence = false
	let fence = ''
	let i = 0
	while (i < lines.length) {
		const line = lines[i]
		const fm = line.match(/^\s*(```+|~~~+)/)
		if (fm && (!inFence || line.trimStart().startsWith(fence))) {
			inFence = !inFence
			fence = inFence ? fm[1] : ''
			out.push({ t: line.replace(/\s+$/, ''), v: true })
			i++
			continue
		}
		if (inFence) {
			out.push({ t: line, v: true }) // 代码块内逐字保留
			i++
			continue
		}
		// 表格：当前行含 `|` 且下一行是对齐分隔行
		if (line.includes('|') && i + 1 < lines.length && isDelimiterRow(lines[i + 1])) {
			const block: string[] = [line, lines[i + 1]]
			let j = i + 2
			while (j < lines.length && lines[j].includes('|') && lines[j].trim() !== '') block.push(lines[j++])
			for (const r of formatTable(block)) out.push({ t: r, v: false })
			i = j
			continue
		}
		out.push({ t: normalizeLine(line), v: false })
		i++
	}
	// 折叠连续空行为最多一个（代码块内除外）；去掉首尾空行；结尾恰好一个换行
	const kept: { t: string; v: boolean }[] = []
	for (const item of out) {
		const prev = kept[kept.length - 1]
		if (item.t === '' && !item.v && prev && prev.t === '' && !prev.v) continue
		kept.push(item)
	}
	while (kept.length && kept[0].t === '') kept.shift()
	while (kept.length && kept[kept.length - 1].t === '') kept.pop()
	return kept.map((x) => x.t).join('\n') + '\n'
}
