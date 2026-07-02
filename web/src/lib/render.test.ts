import { describe, it, expect } from 'vitest'
import { renderMarkdown, renderNote } from './render'
import type { NoteDetail } from './api'

const ID = '0123456789abcdef0123456789abcdef'

function note(body: string, markup = 1): NoteDetail {
	return {
		id: 'n'.repeat(32),
		title: 'T',
		body,
		markup_language: markup,
		parent_id: 'f'.repeat(32),
		created_time: 0,
		updated_time: 0,
		source_url: '',
		is_todo: false,
		todo_completed: false,
	}
}

describe('renderNote (markdown)', () => {
	it('renders headings and inline formatting', () => {
		const html = renderNote(note('# Hi\n\nsome **bold** text'))
		expect(html).toContain('<h1>Hi</h1>')
		expect(html).toContain('<strong>bold</strong>')
	})

	it('rewrites `:/id` image refs to the resource API url', () => {
		const html = renderNote(note(`![cap](:/${ID})`))
		expect(html).toContain(`src="/api/resources/${ID}"`)
		expect(html).not.toContain(':/' + ID)
	})

	it('renders GFM task lists as checkboxes', () => {
		const html = renderNote(note('- [ ] todo\n- [x] done'))
		expect(html).toContain('type="checkbox"')
		expect(html).toContain('checked')
	})

	it('marks internal `:/id` links for in-app handling', () => {
		const html = renderNote(note(`[go](:/${ID})`))
		expect(html).toContain(`data-internal-id="${ID}"`)
	})

	it('adds target/rel to external links', () => {
		const html = renderNote(note('[x](https://example.com)'))
		expect(html).toContain('target="_blank"')
		expect(html).toContain('rel="noopener noreferrer"')
	})
})

describe('renderNote (HTML note, markup_language=2)', () => {
	it('rewrites `:/id` inside raw <img> without markdown processing', () => {
		const html = renderNote(note(`<p><img src=":/${ID}"></p>`, 2))
		expect(html).toContain(`src="/api/resources/${ID}"`)
	})

	it('sanitizes dangerous markup', () => {
		const html = renderNote(note('<img src=x onerror=alert(1)><script>alert(2)</script>', 2))
		expect(html).not.toContain('onerror')
		expect(html.toLowerCase()).not.toContain('<script')
	})
})

describe('renderMarkdown (chat / markdown widget)', () => {
	it('renders markdown with the same sanitize + :/id pipeline', () => {
		const html = renderMarkdown(`**bold** ![img](:/${ID})`)
		expect(html).toContain('<strong>bold</strong>')
		expect(html).toContain(`src="/api/resources/${ID}"`)
	})

	it('sanitizes dangerous markup and tolerates empty input', () => {
		const html = renderMarkdown('<img src=x onerror=alert(1)>')
		expect(html).not.toContain('onerror')
		expect(renderMarkdown('')).toBe('')
	})
})
