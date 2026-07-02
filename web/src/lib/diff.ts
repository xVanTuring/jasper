// 极简行级 diff（写提案确认框用，spec 0.3 §9.5）。零依赖（「别堆包」）：
// 先掐掉公共前后缀（小改动的常态），中段在预算内跑 LCS，超预算退化为整段替换。

export interface DiffLine {
	type: 'same' | 'add' | 'del'
	text: string
}

// 中段 LCS 的规模预算（行数乘积）；超出退化为「整段删除 + 整段新增」（避免大笔记卡 UI）。
const LCS_BUDGET = 250_000

export function diffLines(a: string, b: string): DiffLine[] {
	const al = a.split('\n')
	const bl = b.split('\n')

	// 公共前缀
	let start = 0
	while (start < al.length && start < bl.length && al[start] === bl[start]) start++
	// 公共后缀（不与前缀重叠）
	let endA = al.length
	let endB = bl.length
	while (endA > start && endB > start && al[endA - 1] === bl[endB - 1]) {
		endA--
		endB--
	}

	const out: DiffLine[] = al.slice(0, start).map((text) => ({ type: 'same' as const, text }))
	const midA = al.slice(start, endA)
	const midB = bl.slice(start, endB)

	if (midA.length * midB.length > LCS_BUDGET) {
		out.push(...midA.map((text) => ({ type: 'del' as const, text })))
		out.push(...midB.map((text) => ({ type: 'add' as const, text })))
	} else {
		out.push(...lcsDiff(midA, midB))
	}

	out.push(...al.slice(endA).map((text) => ({ type: 'same' as const, text })))
	return out
}

function lcsDiff(a: string[], b: string[]): DiffLine[] {
	const n = a.length
	const m = b.length
	// dp[i][j] = a[i..] 与 b[j..] 的 LCS 长度
	const dp: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
	for (let i = n - 1; i >= 0; i--) {
		for (let j = m - 1; j >= 0; j--) {
			dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
		}
	}
	const out: DiffLine[] = []
	let i = 0
	let j = 0
	while (i < n && j < m) {
		if (a[i] === b[j]) {
			out.push({ type: 'same', text: a[i] })
			i++
			j++
		} else if (dp[i + 1][j] >= dp[i][j + 1]) {
			out.push({ type: 'del', text: a[i] })
			i++
		} else {
			out.push({ type: 'add', text: b[j] })
			j++
		}
	}
	while (i < n) out.push({ type: 'del', text: a[i++] })
	while (j < m) out.push({ type: 'add', text: b[j++] })
	return out
}
