import { describe, it, expect } from 'vitest'
import {
	evalShowIf,
	filterSections,
	buildRequestBody,
	interpretResult,
	type SettingsSection,
	type SettingsAction,
} from './settingsSchema'

describe('evalShowIf', () => {
	it('undefined condition is always visible', () => {
		expect(evalShowIf(undefined, {})).toBe(true)
	})
	it('equals matches exact value', () => {
		expect(evalShowIf({ field: 's', equals: 'local' }, { s: 'local' })).toBe(true)
		expect(evalShowIf({ field: 's', equals: 'local' }, { s: 'webdav' })).toBe(false)
	})
	it('in / not_in match membership', () => {
		expect(evalShowIf({ field: 'm', in: ['a', 'b'] }, { m: 'a' })).toBe(true)
		expect(evalShowIf({ field: 'm', in: ['a', 'b'] }, { m: 'c' })).toBe(false)
		// not_in: 插件 provider key（非 local/webdav）→ 显示 provider-config
		expect(evalShowIf({ field: 's', not_in: ['local', 'webdav'] }, { s: 'plugin:x:y' })).toBe(true)
		expect(evalShowIf({ field: 's', not_in: ['local', 'webdav'] }, { s: 'local' })).toBe(false)
	})
	it('truthy treats empty string / false as hidden', () => {
		expect(evalShowIf({ field: 'p', truthy: true }, { p: true })).toBe(true)
		expect(evalShowIf({ field: 'p', truthy: true }, { p: 'anthropic' })).toBe(true)
		expect(evalShowIf({ field: 'p', truthy: true }, { p: '' })).toBe(false)
		expect(evalShowIf({ field: 'p', truthy: true }, { p: false })).toBe(false)
	})
})

// 用非 i18n 键的字面标题：resolveLabel 对未知键回退原串，故搜索行为与语言无关。
const sections: SettingsSection[] = [
	{
		id: 'data-source',
		title_key: 'Data source',
		icon: 'folder',
		fields: [{ key: 'local_path', type: 'text', label_key: 'Folder path' }],
		search_keys: ['WebDAV'],
	},
	{
		id: 'ai',
		title_key: 'AI',
		icon: 'sparkles',
		fields: [{ key: 'api_key', type: 'secret', label_key: 'API key' }],
	},
]

describe('filterSections', () => {
	it('empty query returns all', () => {
		expect(filterSections(sections, '').map((s) => s.id)).toEqual(['data-source', 'ai'])
		expect(filterSections(sections, '  ').map((s) => s.id)).toEqual(['data-source', 'ai'])
	})
	it('matches section title case-insensitively', () => {
		expect(filterSections(sections, 'data').map((s) => s.id)).toEqual(['data-source'])
	})
	it('matches a field label', () => {
		expect(filterSections(sections, 'api key').map((s) => s.id)).toEqual(['ai'])
	})
	it('matches search_keys', () => {
		expect(filterSections(sections, 'webdav').map((s) => s.id)).toEqual(['data-source'])
	})
	it('no match returns empty', () => {
		expect(filterSections(sections, 'zzz')).toEqual([])
	})
})

const connect: SettingsAction = {
	id: 'connect',
	label_key: 'Connect',
	request: { method: 'PUT', url: '/api/config', convention: 'config-result' },
	on_success: 'reload',
}
const save: SettingsAction = {
	id: 'save',
	label_key: 'Save',
	request: { method: 'PUT', url: '/api/auth/settings', convention: 'status' },
	on_success: 'relogin',
}
const clear: SettingsAction = {
	id: 'clear',
	label_key: 'Clear',
	request: { method: 'PUT', url: '/api/auth/settings', convention: 'status', extra: { clear_password: true } },
	submit: false,
}

describe('buildRequestBody', () => {
	it('data-source local → connect payload with create_new derived', () => {
		const body = buildRequestBody('data-source', connect, {
			create_new: 'existing',
			source_type: 'local',
			local_path: '/notes',
		})
		expect(body).toMatchObject({
			source_type: 'local',
			local_path: '/notes',
			plugin_id: '',
			plugin_storage: '',
			create_new: false,
			read_only: false,
		})
	})
	it('data-source plugin provider → splits key + nests plugin_config + create_new', () => {
		const body = buildRequestBody('data-source', connect, {
			create_new: 'new',
			source_type: 'plugin:webdav-storage:webdav',
			plugin_config: { url: 'https://x/' },
		})
		expect(body).toMatchObject({
			source_type: 'plugin',
			plugin_id: 'webdav-storage',
			plugin_storage: 'webdav',
			plugin_config: { url: 'https://x/' },
			create_new: true,
		})
	})
	it('non-data-source section posts field values as-is', () => {
		const values = { passwordless_read: true, list_mode: 'whitelist', folder_list: ['a'], password: 'x' }
		expect(buildRequestBody('access-control', save, values)).toEqual(values)
	})
	it('submit:false action sends only request.extra', () => {
		expect(buildRequestBody('access-control', clear, { password: 'x', list_mode: 'none' })).toEqual({
			clear_password: true,
		})
	})
})

describe('interpretResult', () => {
	it('config-result reads body.ok', () => {
		expect(interpretResult('config-result', true, { ok: true, notes: 3 })).toEqual({ ok: true })
		expect(interpretResult('config-result', true, { ok: false, error: 'bad path' })).toEqual({
			ok: false,
			error: 'bad path',
		})
	})
	it('status uses HTTP ok, extracting message/error on failure', () => {
		expect(interpretResult('status', true, null)).toEqual({ ok: true })
		expect(interpretResult('status', false, { message: 'invalid' })).toEqual({ ok: false, error: 'invalid' })
		expect(interpretResult('status', false, { error: 'boom' })).toEqual({ ok: false, error: 'boom' })
	})
})
