// 单测桩：替身 wasm-pack 产物（src/wasm-pkg/，由 build:wasm 生成、已 gitignore）。
// CI 的 web-unit 不构建 wasm，故 api.ts 里对 `../wasm-pkg/jasper_wasm.js` 的懒加载
// import() 会让 Vite 的 import-analysis 找不到文件而报错。vitest.config.ts 把该
// 说明符 alias 到本桩，测试即可编译 api.ts。DEMO 分支在 VITE_DEMO!=1 时是死代码，
// 桩的运行时实现永不被调用（此处仅需类型/形状占位）。
export default async function init(): Promise<void> {}
export class Demo {
	folders(): string {
		return '[]'
	}
	notes(_f: string): string {
		return '[]'
	}
	note(_id: string): string {
		return 'null'
	}
	search(_q: string): string {
		return '[]'
	}
}
