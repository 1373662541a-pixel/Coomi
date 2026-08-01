import { WsTransport } from './wsTransport'
import { DemoTransport } from './demoTransport'
import { isDemoMode } from './demoMode'
import type { Transport } from './transport'

export type { Transport, ConnectionState } from './transport'

export function createTransport(sessionId: string, wsUrl?: string): Transport {
  // 演示模式不碰网络：引擎没起来也要能看界面。界面上有「演示」标记。
  if (isDemoMode()) return new DemoTransport()
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const base = wsUrl || import.meta.env.VITE_ENGINE_WS || `${protocol}//${location.host}`
  const url = base.includes('/ws/') ? base : `${base.replace(/\/$/, '')}/ws/session/${sessionId}`
  return new WsTransport({ url })
}
