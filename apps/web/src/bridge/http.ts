// HTTP API 基址推导。
// 生产：web 由 bridge 同源伺服 → 用相对路径（base=""）。
// 开发：若设了 VITE_ENGINE_WS（ws://host:port），推导出 http://host:port。
function deriveBase(): string {
  const ws = import.meta.env.VITE_ENGINE_WS as string | undefined
  if (!ws) return '' // same-origin
  try {
    const u = new URL(ws)
    const proto = u.protocol === 'wss:' ? 'https:' : 'http:'
    return `${proto}//${u.host}`
  } catch {
    return ''
  }
}

export const API_BASE = deriveBase()

export async function apiGet<T>(path: string): Promise<T> {
  const r = await fetch(`${API_BASE}${path}`, { headers: { Accept: 'application/json' } })
  if (!r.ok) throw new Error(`GET ${path} → ${r.status}`)
  return r.json() as Promise<T>
}

export async function apiSend<T>(path: string, method: 'POST' | 'DELETE', body?: unknown): Promise<T> {
  const r = await fetch(`${API_BASE}${path}`, {
    method,
    headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!r.ok) {
    let msg = `${method} ${path} → ${r.status}`
    try { const e = await r.json(); if (e?.error) msg = e.error } catch { /* ignore */ }
    throw new Error(msg)
  }
  return r.json() as Promise<T>
}
