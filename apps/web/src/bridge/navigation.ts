import type { Router } from 'vue-router'
import { closeTopOverlay } from './overlayStack'

type BackFallback = 'dashboard' | string

function hasRouterHistory(): boolean {
  return Boolean(window.history.state?.back)
}

export function goBack(router: Router, fallback: BackFallback): void {
  if (fallback === 'dashboard' && window.CoomiAndroid?.openDashboard) {
    window.CoomiAndroid.openDashboard()
    return
  }
  if (hasRouterHistory()) {
    router.back()
    return
  }
  void router.replace(fallback === 'dashboard' ? '/' : fallback)
}

export function installSystemBackHandler(router: Router): void {
  window.__coomiHandleSystemBack = () => {
    if (closeTopOverlay()) return true
    const route = router.currentRoute.value.path
    if (route === '/') return false
    if (route === '/persona') goBack(router, '/settings')
    else if (
      route === '/hooks'
      || route === '/memory'
      || route === '/runtime'
      || route === '/files'
      || route === '/catalog'
      || route === '/providers'
      || route.startsWith('/providers/')
    ) goBack(router, 'dashboard')
    else goBack(router, '/')
    return true
  }
}
