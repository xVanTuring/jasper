import { mount } from 'svelte'
import './app.css'
import './icons.css'
// 内置示例主题（打包内置；将来插件主题由宿主动态加载，见 docs/plugin-design.md §5.1）
import './themes/nord.css'
import './themes/solarized.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
