// Joplin 笔记渲染器。
// markdown-it 内核 + 成熟插件（对齐 Joplin 的扩展集），仅保留 Joplin 专有的
// 「资源链接 :/id 改写」为自定义逻辑。代码高亮用 highlight.js，输出经 DOMPurify 净化。

import MarkdownIt from 'markdown-it'
// 只引入常用 ~37 种语言（而非全部 ~190），大幅减小打包体积。
import hljs from 'highlight.js/lib/common'
import DOMPurify from 'dompurify'

import taskLists from 'markdown-it-task-lists'
import katexPlugin from '@vscode/markdown-it-katex'
import mark from 'markdown-it-mark'
import footnote from 'markdown-it-footnote'
import sub from 'markdown-it-sub'
import sup from 'markdown-it-sup'
import ins from 'markdown-it-ins'
import deflist from 'markdown-it-deflist'
import abbr from 'markdown-it-abbr'
import { full as emoji } from 'markdown-it-emoji'
import multimdTable from 'markdown-it-multimd-table'

import { api, type NoteDetail } from './api'

import 'highlight.js/styles/github-dark.css'
import 'katex/dist/katex.min.css'

const md = new MarkdownIt({
  html: true, // Joplin 始终允许 HTML
  linkify: true,
  typographer: true,
  breaks: false,
  highlight(str, lang) {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return '<pre class="hljs"><code>' + hljs.highlight(str, { language: lang }).value + '</code></pre>'
      } catch {
        /* fall through */
      }
    }
    return '<pre class="hljs"><code>' + md.utils.escapeHtml(str) + '</code></pre>'
  },
})

// 兼容 CJS/ESM 默认导出差异：打包后有的插件是函数本身，有的被包成 { default: fn }。
const P = (m: any): any => (typeof m === 'function' ? m : m?.default ?? m)

md.use(P(taskLists), { label: true }) // 只读：默认 checkbox 禁用
  .use(P(katexPlugin))
  .use(P(mark))
  .use(P(footnote))
  .use(P(sub))
  .use(P(sup))
  .use(P(ins))
  .use(P(deflist))
  .use(P(abbr))
  .use(P(emoji))
  .use(P(multimdTable), { multiline: true, rowspan: true, headerless: true })

// ---------- 资源链接 :/id（Joplin 专有，来源 renderer/urlUtils.js:3）----------
function parseResourceUrl(url: string): { id: string } | null {
  const m = /^(?:joplin:\/\/|:\/)([0-9a-zA-Z]{32})(?:#.*)?$/.exec(url.trim())
  return m ? { id: m[1] } : null
}

// 对最终 HTML 统一改写 :/id 引用。同时覆盖 markdown 笔记和 HTML 剪藏笔记，
// 避免 HTML 笔记里的 <img src=":/id"> 被浏览器当相对路径请求成 /:/id（404）。
function rewriteResourceRefs(html: string): string {
  const doc = new DOMParser().parseFromString(html, 'text/html')
  doc.querySelectorAll('img, source, audio, video').forEach((el) => {
    const p = parseResourceUrl(el.getAttribute('src') || '')
    if (p) el.setAttribute('src', api.resourceUrl(p.id))
  })
  doc.querySelectorAll('a[href]').forEach((el) => {
    const href = el.getAttribute('href') || ''
    const p = parseResourceUrl(href)
    if (p) {
      // 内部链接（笔记或资源）：交给应用处理点击
      el.setAttribute('href', '#')
      el.setAttribute('data-internal-id', p.id)
    } else if (/^https?:\/\//i.test(href)) {
      el.setAttribute('target', '_blank')
      el.setAttribute('rel', 'noopener noreferrer')
    }
  })
  return doc.body.innerHTML
}

// ---------- 对外渲染 ----------
const SANITIZE_OPTS = {
  ADD_ATTR: ['target', 'data-internal-id', 'checked', 'disabled', 'rel'],
  ADD_TAGS: ['input'],
}

/** 渲染一篇笔记为安全 HTML。markup_language: 1=Markdown, 2=HTML。 */
export function renderNote(detail: NoteDetail): string {
  const raw = detail.markup_language === 2 ? detail.body : md.render(detail.body)
  return DOMPurify.sanitize(rewriteResourceRefs(raw), SANITIZE_OPTS)
}
