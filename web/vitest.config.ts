import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { svelteTesting } from '@testing-library/svelte/vite'

// 前端单元测试（Vitest）。svelte 插件让 `.svelte` / `.svelte.ts`（runes）能被编译；
// svelteTesting() 设定 browser 解析条件 + 组件自动清理。jsdom 提供 DOMParser/localStorage 等。
export default defineConfig({
	plugins: [svelte(), svelteTesting()],
	test: {
		environment: 'jsdom',
		include: ['src/**/*.test.ts'],
		globals: false,
		restoreMocks: true,
	},
})
