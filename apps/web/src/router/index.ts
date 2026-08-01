import { createRouter, createWebHashHistory } from 'vue-router'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'chat', component: () => import('@/views/ChatView.vue') },
    { path: '/sessions', name: 'sessions', component: () => import('@/views/SessionsView.vue') },
    { path: '/settings', name: 'settings', component: () => import('@/views/SettingsView.vue') },
    { path: '/providers', name: 'providers', component: () => import('@/views/ProvidersView.vue') },
    { path: '/runtime', name: 'runtime', component: () => import('@/views/RuntimeView.vue') },
  ],
})
