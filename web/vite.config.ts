import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  server: {
    // 绑 IPv4，避免 Vite 把 localhost 解析成 ::1 而只监听 [::1]，
    // 导致用 127.0.0.1:5173 访问被拒（与后端/文档统一用 127.0.0.1）。
    host: '127.0.0.1',
    // 开发期把 /api 转发到本机后端（cargo run，默认 127.0.0.1:27583）；
    // 否则 /api/* 命中 Vite 返回 index.html，前端 res.json() 解析 HTML 报错。
    proxy: {
      '/api': 'http://127.0.0.1:27583',
    },
  },
})
