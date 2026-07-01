import { defineConfig } from 'vitest/config'
import { fileURLToPath } from 'node:url'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { svelteTesting } from '@testing-library/svelte/vite'

// 前端单元测试（Vitest）。svelte 插件让 `.svelte` / `.svelte.ts`（runes）能被编译；
// svelteTesting() 设定 browser 解析条件 + 组件自动清理。jsdom 提供 DOMParser/localStorage 等。
export default defineConfig({
	plugins: [svelte(), svelteTesting()],
	resolve: {
		alias: {
			// api.ts 懒加载 wasm-pack 产物（src/wasm-pkg/，由 build:wasm 生成、已 gitignore）。
			// CI 单测不构建 wasm，Vite import-analysis 会因找不到文件而失败；alias 到桩即可编译。
			'../wasm-pkg/jasper_wasm.js': fileURLToPath(new URL('./src/wasm-pkg-stub.ts', import.meta.url)),
		},
	},
	test: {
		environment: 'jsdom',
		include: ['src/**/*.test.ts'],
		globals: false,
		restoreMocks: true,
	},
})
