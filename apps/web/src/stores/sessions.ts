import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { isDemoMode } from '@/bridge/demoMode'
import type { Timelineitem } from './viewModel'

/**
 * 会话历史（纯本机）。
 *
 * bridge 没有「列出会话」接口，引擎侧 SessionManager 也只把会话放在内存里
 * （coomi/engine/session.py:110），所以历史列表只能由前端自己维护：
 *   - 元数据存 localStorage，抽屉据此分组渲染；
 *   - 时间线也存一份，重开旧会话时先把本机记录铺回来；
 *   - 重连用同一个 sessionId，引擎进程没重启的话上下文是真的接上了，
 *     重启过就只剩本机这份记录 —— 这一点由 ChatView 明确提示用户。
 */

const META_KEY = 'coomi.sessions.v1'
const TRANSCRIPT_PREFIX = 'coomi.transcript.'
/** 只给最近的若干会话留时间线，避免把 localStorage 撑爆。 */
const KEEP_TRANSCRIPTS = 12
const MAX_ITEMS_PER_TRANSCRIPT = 400

export interface SessionMeta {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  turns: number
  pinned: boolean
  /** 创建该会话时的工作目录；用于把不同项目的会话隔离开。 */
  cwd?: string
}

export interface SessionGroup {
  label: string
  items: SessionMeta[]
}

function readMetas(): SessionMeta[] {
  try {
    const raw = localStorage.getItem(META_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed)
      ? (parsed as SessionMeta[]).filter(meta => meta.turns > 0 || meta.title !== '新对话')
      : []
  } catch {
    return []
  }
}

function dayStart(offsetDays = 0): number {
  const d = new Date()
  d.setHours(0, 0, 0, 0)
  return d.getTime() - offsetDays * 86400000
}

export function formatSessionTime(ts: number): string {
  const d = new Date(ts)
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
  if (ts >= dayStart()) return hm
  if (ts >= dayStart(1)) return `昨天 ${hm}`
  return `${d.getMonth() + 1}月${d.getDate()}日`
}

export const useSessionsStore = defineStore('sessions', () => {
  const metas = ref<SessionMeta[]>(readMetas())
  const query = ref('')
  /** 引擎当前工作目录（来自 /api/runtime/health），用于会话按项目隔离。 */
  const currentCwd = ref('')

  function setCurrentCwd(cwd: string) {
    currentCwd.value = cwd
  }

  const sorted = computed(() =>
    [...metas.value].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt)
  )

  const filtered = computed(() => {
    const q = query.value.trim().toLowerCase()
    if (!q) return sorted.value
    return sorted.value.filter(m => m.title.toLowerCase().includes(q))
  })

  /** 置顶 / 今天 / 昨天 / 7 天内 / 更早 / 其它目录 —— 空组不出现。 */
  const groups = computed<SessionGroup[]>(() => {
    const buckets: SessionGroup[] = [
      { label: '置顶', items: [] },
      { label: '今天', items: [] },
      { label: '昨天', items: [] },
      { label: '7 天内', items: [] },
      { label: '更早', items: [] },
      { label: '其它目录', items: [] },
    ]
    const today = dayStart()
    const yesterday = dayStart(1)
    const week = dayStart(7)
    const current = currentCwd.value
    for (const m of filtered.value) {
      if (m.pinned) buckets[0].items.push(m)
      // 会话属于其它工作目录时归入「其它目录」，避免把别的项目的会话混进当前项目。
      // cwd 为空的是旧数据，按当前项目对待。
      else if (current && m.cwd && m.cwd !== current) buckets[5].items.push(m)
      else if (m.updatedAt >= today) buckets[1].items.push(m)
      else if (m.updatedAt >= yesterday) buckets[2].items.push(m)
      else if (m.updatedAt >= week) buckets[3].items.push(m)
      else buckets[4].items.push(m)
    }
    return buckets.filter(b => b.items.length > 0)
  })

  function persist() {
    try {
      localStorage.setItem(META_KEY, JSON.stringify(metas.value))
    } catch {
      /* 配额满就放弃写入，不影响会话本身 */
    }
  }

  function find(id: string): SessionMeta | undefined {
    return metas.value.find(m => m.id === id)
  }

  /**
   * 演示模式下建/动会话只留在内存里：预览不该往真实历史里塞条目。
   * 用户主动的重命名 / 置顶 / 删除仍然照常落盘。
   */
  function persistMeta() {
    if (!isDemoMode()) persist()
  }

  /** 第一条用户消息就是标题，截断到一行能放下的长度。 */
  function deriveTitle(text: string): string {
    const t = text.replace(/\s+/g, ' ').trim()
    return t.length > 42 ? t.slice(0, 42) + '…' : t || '新对话'
  }

  function ensure(id: string, title = '新对话'): SessionMeta {
    let m = find(id)
    if (!m) {
      m = { id, title, createdAt: Date.now(), updatedAt: Date.now(), turns: 0, pinned: false, cwd: currentCwd.value || undefined }
      metas.value.unshift(m)
      persistMeta()
    }
    return m
  }

  function touch(id: string, patch: Partial<Pick<SessionMeta, 'title' | 'turns'>> = {}) {
    const m = ensure(id)
    if (patch.title) m.title = patch.title
    if (patch.turns != null) m.turns = patch.turns
    m.updatedAt = Date.now()
    persistMeta()
  }

  function rename(id: string, title: string) {
    const m = find(id)
    if (!m) return
    m.title = title.trim() || m.title
    persist()
  }

  function togglePin(id: string) {
    const m = find(id)
    if (!m) return
    m.pinned = !m.pinned
    persist()
  }

  function remove(id: string) {
    metas.value = metas.value.filter(m => m.id !== id)
    try {
      localStorage.removeItem(TRANSCRIPT_PREFIX + id)
    } catch {
      /* ignore */
    }
    persist()
  }

  /** Migrate pre-Rust session ids while preserving the local transcript and metadata. */
  function migrateId(oldId: string, newId: string): string {
    const meta = find(oldId)
    if (!meta) return newId
    meta.id = newId
    try {
      const transcript = localStorage.getItem(TRANSCRIPT_PREFIX + oldId)
      if (transcript) localStorage.setItem(TRANSCRIPT_PREFIX + newId, transcript)
      localStorage.removeItem(TRANSCRIPT_PREFIX + oldId)
    } catch {
      /* Keep the migrated metadata even if WebView storage is temporarily unavailable. */
    }
    persist()
    return newId
  }

  /** 只留最近 KEEP_TRANSCRIPTS 份时间线，老的元数据保留、正文丢弃。 */
  function pruneTranscripts() {
    const keep = new Set(sorted.value.slice(0, KEEP_TRANSCRIPTS).map(m => m.id))
    for (const m of metas.value) {
      if (keep.has(m.id)) continue
      try {
        localStorage.removeItem(TRANSCRIPT_PREFIX + m.id)
      } catch {
        /* ignore */
      }
    }
  }

  function saveTranscript(id: string, items: Timelineitem[]) {
    if (items.length === 0) return
    const tail = items.slice(-MAX_ITEMS_PER_TRANSCRIPT)
    try {
      localStorage.setItem(TRANSCRIPT_PREFIX + id, JSON.stringify(tail))
      pruneTranscripts()
    } catch {
      // 配额满：清掉最旧的正文再试一次，仍失败就算了
      pruneTranscripts()
      try {
        localStorage.setItem(TRANSCRIPT_PREFIX + id, JSON.stringify(tail))
      } catch {
        /* ignore */
      }
    }
  }

  function loadTranscript(id: string): Timelineitem[] {
    try {
      const raw = localStorage.getItem(TRANSCRIPT_PREFIX + id)
      if (!raw) return []
      const parsed = JSON.parse(raw)
      return Array.isArray(parsed) ? (parsed as Timelineitem[]) : []
    } catch {
      return []
    }
  }

  function clearAll() {
    for (const m of metas.value) {
      try {
        localStorage.removeItem(TRANSCRIPT_PREFIX + m.id)
      } catch {
        /* ignore */
      }
    }
    metas.value = []
    persist()
  }

  return {
    metas, query, sorted, filtered, groups, currentCwd, setCurrentCwd,
    ensure, touch, rename, togglePin, remove, find, deriveTitle,
    saveTranscript, loadTranscript, migrateId, clearAll,
  }
})
