import { createRouter, createWebHashHistory } from 'vue-router'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'chat', component: () => import('@/views/ChatView.vue') },
    { path: '/sessions', name: 'sessions', component: () => import('@/views/SessionsView.vue') },
    { path: '/settings', name: 'settings', component: () => import('@/views/SettingsView.vue') },
    { path: '/persona', name: 'persona', component: () => import('@/views/PersonaView.vue') },
    { path: '/providers', name: 'providers', component: () => import('@/views/ProvidersView.vue') },
    { path: '/providers/new', name: 'provider-new', component: () => import('@/views/ProviderDetailView.vue') },
    { path: '/providers/:id', name: 'provider-detail', component: () => import('@/views/ProviderDetailView.vue') },
    { path: '/runtime', name: 'runtime', component: () => import('@/views/RuntimeView.vue') },
    { path: '/catalog', name: 'catalog', component: () => import('@/views/CatalogView.vue') },
    { path: '/hooks', name: 'hooks', component: () => import('@/views/HooksView.vue') },
    { path: '/memory', name: 'memory', component: () => import('@/views/MemoryView.vue') },
    { path: '/files', name: 'files', component: () => import('@/views/FileManagerView.vue') },
  ],
})
