import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import './styles/global.css'

/** 桌面浏览器兜底：跟随系统深色模式写入 <html data-theme>；
 *  Android WebView 内由 CoomiActivity 经 JS 桥设置同一属性（优先级更高，不会被覆盖）。 */
function initTheme() {
  const apply = (dark: boolean) => {
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
  }
  if (window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    apply(mq.matches)
    const onChange = (e: MediaQueryListEvent) => apply(e.matches)
    if (typeof mq.addEventListener === 'function') mq.addEventListener('change', onChange)
    else mq.addListener(onChange)
  }
}

initTheme()
createApp(App).use(createPinia()).use(router).mount('#app')
