// 多语言文案字典（中/英）。键名用 `区域.名称`，`{x}` 为插值占位符。
// zh 为基准；`const en: typeof zh` 强制 en 含且仅含相同键（漏译会编译报错）。
// 取词/插值见 i18n.svelte.ts 的 t()。

export type Locale = 'zh' | 'en'

const zh = {
	// 通用
	'common.close': '关闭',
	'common.save': '保存',
	'common.cancel': '取消',
	'common.rename': '重命名',
	'common.delete': '删除',
	'common.loading': '加载中…',
	'common.untitled': '(无标题)',
	'common.unnamed': '(未命名)',
	'common.langTitle': '切换语言 / Switch language',
	'common.themeTitle': '主题：{mode}（点击切换）',
	'theme.auto': '跟随系统',
	'theme.light': '浅色',
	'theme.dark': '深色',
	'theme.pick': '主题：{mode}',

	// 顶栏 / 三栏
	'topbar.search': '搜索笔记…',
	'topbar.resources': '资源管理',
	'topbar.settings': '设置',
	'pane.notebooks': '笔记本',
	'pane.newNote': '在当前笔记本新建笔记',
	'list.searchPrefix': '搜索：{q}',
	'list.empty': '没有笔记',
	'app.loadFailed': '加载失败：{err}',
	'note.newNote': '新笔记',

	// 演示横幅（main 段含 <b> 标记，用 {@html} 渲染）
	'demo.banner':
		'<b>演示预览</b> · 全程在浏览器内由 Rust&nbsp;→&nbsp;WASM 运行，<b>无后端</b>。可浏览笔记本/笔记、Markdown 渲染（代码 · 表格 · 公式 · 任务清单）、全文搜索。',
	'demo.bannerDim': '编辑、图片资源、WebDAV / 本地读写为完整版（本地 server）能力。',

	// 设置 / 首次向导
	'settings.welcomeTitle': '欢迎使用 joplin-lite',
	'settings.title': '设置',
	'settings.firstHint': '首次使用，请选择笔记库。',
	'settings.useExisting': '使用现有笔记库',
	'settings.createNew': '新建笔记库',
	'settings.existingDesc': '连接到已有的 Joplin 同步目录 / WebDAV，读取并编辑已有笔记。',
	'settings.newDesc': '在指定位置创建一个空的新库（会写入 info.json）。',
	'settings.local': '本地文件夹',
	'settings.webdav': 'WebDAV',
	'settings.folderPath': '文件夹路径',
	'settings.localPhNew': '/Users/你/新笔记库',
	'settings.localPhExisting': '/Users/你/Joplin同步目录',
	'settings.localTip': '服务运行在本机，请填写该机器上的绝对路径。',
	'settings.webdavUrl': 'WebDAV 地址',
	'settings.webdavUrlPh': 'https://host/remote.php/dav/files/用户/Joplin',
	'settings.username': '用户名',
	'settings.password': '密码',
	'settings.connectFailed': '连接失败',
	'settings.connecting': '连接中…',
	'settings.createConnect': '创建并连接',
	'settings.connect': '连接',

	// 笔记视图
	'note.saving': '保存中…',
	'note.saved': '已保存',
	'note.saveFailed': '保存失败',
	'note.read': '阅读',
	'note.edit': '编辑',
	'note.delete': '删除',
	'note.toSource': '源码',
	'note.toRich': '富文本',
	'note.titlePlaceholder': '标题',
	'note.updatedAt': '更新于 {time}',
	'note.source': '来源',
	'note.confirmDelete': '确定删除「{title}」？',
	'note.placeholder': '选择一篇笔记查看',

	// 编辑器
	'editor.attach': '附件',
	'editor.hint': '可直接粘贴或拖拽图片',
	'editor.uploading': '上传中…（{n}）',
	'editor.uploadFailed': '上传失败',
	'editor.wysiwygFailed': '富文本编辑器加载失败',

	// 笔记本树
	'tree.toggle': '展开/折叠',

	// 资源管理
	'res.title': '资源管理',
	'res.count': '{n} 个资源',
	'res.orphans': '{n} 个孤儿',
	'res.cleanup': '清理孤儿（{n}）',
	'res.empty': '还没有资源。在编辑笔记时粘贴/拖拽图片或用「附件」上传。',
	'res.openNewTab': '在新标签打开',
	'res.unknownType': '未知类型',
	'res.usedBy': '被 {n} 篇引用',
	'res.unused': '未被引用',
	'res.confirmDeleteUsed':
		'资源「{title}」被 {n} 篇笔记引用，删除后这些笔记的图片/附件会失效。仍要删除？',
	'res.confirmDeleteUnused': '删除未被引用的资源「{title}」？',
	'res.confirmCleanup': '将删除 {n} 个未被任何笔记引用的资源，无法撤销。继续？',

	// API 错误（抛出后由界面显示）
	'api.uploadFailed': '上传失败',
	'api.deleteResFailed': '删除资源失败',
}

const en: typeof zh = {
	'common.close': 'Close',
	'common.save': 'Save',
	'common.cancel': 'Cancel',
	'common.rename': 'Rename',
	'common.delete': 'Delete',
	'common.loading': 'Loading…',
	'common.untitled': '(Untitled)',
	'common.unnamed': '(Unnamed)',
	'common.langTitle': '切换语言 / Switch language',
	'common.themeTitle': 'Theme: {mode}',
	'theme.auto': 'Auto',
	'theme.light': 'Light',
	'theme.dark': 'Dark',
	'theme.pick': 'Theme: {mode}',

	'topbar.search': 'Search notes…',
	'topbar.resources': 'Resources',
	'topbar.settings': 'Settings',
	'pane.notebooks': 'Notebooks',
	'pane.newNote': 'New note in this notebook',
	'list.searchPrefix': 'Search: {q}',
	'list.empty': 'No notes',
	'app.loadFailed': 'Load failed: {err}',
	'note.newNote': 'New note',

	'demo.banner':
		'<b>Demo preview</b> · Runs entirely in your browser via Rust&nbsp;→&nbsp;WASM, <b>no backend</b>. Browse notebooks/notes, Markdown rendering (code · tables · math · task lists), and full-text search.',
	'demo.bannerDim':
		'Editing, image resources, and WebDAV / local read-write are full-version (local server) features.',

	'settings.welcomeTitle': 'Welcome to joplin-lite',
	'settings.title': 'Settings',
	'settings.firstHint': 'First run — pick a note library.',
	'settings.useExisting': 'Use existing library',
	'settings.createNew': 'Create new library',
	'settings.existingDesc':
		'Connect to an existing Joplin sync directory / WebDAV to read and edit existing notes.',
	'settings.newDesc': 'Create an empty new library at the chosen location (writes info.json).',
	'settings.local': 'Local folder',
	'settings.webdav': 'WebDAV',
	'settings.folderPath': 'Folder path',
	'settings.localPhNew': '/Users/you/new-library',
	'settings.localPhExisting': '/Users/you/Joplin-sync-dir',
	'settings.localTip': 'The server runs on this machine — enter an absolute path on it.',
	'settings.webdavUrl': 'WebDAV URL',
	'settings.webdavUrlPh': 'https://host/remote.php/dav/files/user/Joplin',
	'settings.username': 'Username',
	'settings.password': 'Password',
	'settings.connectFailed': 'Connection failed',
	'settings.connecting': 'Connecting…',
	'settings.createConnect': 'Create & connect',
	'settings.connect': 'Connect',

	'note.saving': 'Saving…',
	'note.saved': 'Saved',
	'note.saveFailed': 'Save failed',
	'note.read': 'Read',
	'note.edit': 'Edit',
	'note.delete': 'Delete',
	'note.toSource': 'Source',
	'note.toRich': 'Rich text',
	'note.titlePlaceholder': 'Title',
	'note.updatedAt': 'Updated {time}',
	'note.source': 'Source',
	'note.confirmDelete': 'Delete “{title}”?',
	'note.placeholder': 'Select a note to view',

	'editor.attach': 'Attach',
	'editor.hint': 'Paste or drag images directly',
	'editor.uploading': 'Uploading… ({n})',
	'editor.uploadFailed': 'Upload failed',
	'editor.wysiwygFailed': 'Failed to load the rich-text editor',

	'tree.toggle': 'Expand/collapse',

	'res.title': 'Resources',
	'res.count': '{n} resources',
	'res.orphans': '{n} orphan(s)',
	'res.cleanup': 'Clean orphans ({n})',
	'res.empty': 'No resources yet. Paste/drag images while editing, or use “Attach”.',
	'res.openNewTab': 'Open in new tab',
	'res.unknownType': 'Unknown type',
	'res.usedBy': 'Used by {n}',
	'res.unused': 'Unused',
	'res.confirmDeleteUsed':
		'Resource “{title}” is used by {n} note(s); deleting it will break their images/attachments. Delete anyway?',
	'res.confirmDeleteUnused': 'Delete unused resource “{title}”?',
	'res.confirmCleanup':
		'This will delete {n} resource(s) not referenced by any note. This cannot be undone. Continue?',

	'api.uploadFailed': 'Upload failed',
	'api.deleteResFailed': 'Failed to delete resource',
}

export type MsgKey = keyof typeof zh
export const messages: Record<Locale, Record<MsgKey, string>> = { zh, en }
