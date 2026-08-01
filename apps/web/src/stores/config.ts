import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { PermissionMode } from '@/protocol/commands'
import { apiGet, apiSend } from '@/bridge/http'

export interface ProviderConfig {
  id: string; name: string; apiKeyMasked: string; hasKey?: boolean
  models: string[]; baseUrl?: string
  type?: string; model?: string; fastModel?: string | null; toolProtocol?: string
  contextWindow?: number
  supportsWebSearch?: boolean
  active?: boolean
}

export interface ProviderInput {
  id: string; name: string; apiKey: string; models: string[]
  baseUrl?: string; type?: string; toolProtocol?: string; contextWindow?: number
  fastModel?: string | null; activate?: boolean; supportsWebSearch?: boolean
}

export const PERMISSION_MODES: { mode: PermissionMode; label: string; desc: string }[] = [
  { mode: 'ask', label: '询问', desc: '每个写入/破坏性操作前都确认' },
  { mode: 'auto', label: '自动', desc: '读写自动放行，仅破坏性需确认' },
  { mode: 'full', label: '放行', desc: '全部自动执行（仅信任场景）' },
]

// 浏览器独立开发时的兜底数据（后端不可达时使用）
const MOCK_PROVIDERS: ProviderConfig[] = [
  { id: 'openai', name: 'OpenAI', apiKeyMasked: '****a1b2', hasKey: true, models: ['gpt-4o', 'gpt-4o-mini'], baseUrl: 'https://api.openai.com/v1' },
  { id: 'anthropic', name: 'Anthropic', apiKeyMasked: '****9f3c', hasKey: true, models: ['claude-sonnet-4', 'claude-opus-4'] },
]

export const useConfigStore = defineStore('config', () => {
  const savedPermission = localStorage.getItem('coomi.permissionMode') as PermissionMode | null
  const permissionMode = ref<PermissionMode>(['ask', 'auto', 'full'].includes(savedPermission ?? '') ? savedPermission! : 'ask')
  const planMode = ref(false)

  const providers = ref<ProviderConfig[]>([])
  const activeId = ref('')
  const loading = ref(false)
  const usingMock = ref(false)
  const lastError = ref<string | null>(null)

  const currentProviderId = ref('')
  const currentModel = ref('')
  const currentProvider = computed(() => providers.value.find(p => p.id === currentProviderId.value) ?? null)

  function applyList(list: ProviderConfig[], active: string) {
    providers.value = list
    activeId.value = active
    // 同步当前选择：优先 active，其次第一个
    const sel = list.find(p => p.id === active) ?? list[0]
    if (sel) {
      const savedProvider = localStorage.getItem('coomi.providerId')
      const savedModel = localStorage.getItem('coomi.model')
      const saved = list.find(p => p.id === savedProvider && p.models.includes(savedModel ?? ''))
      currentProviderId.value = saved?.id ?? sel.id
      currentModel.value = savedModel && saved ? savedModel : (sel.model || sel.models[0] || '')
    }
  }

  /** 从后端拉取 Provider 列表；失败则用 mock 兜底（浏览器独立开发）。 */
  async function fetchProviders() {
    loading.value = true
    lastError.value = null
    try {
      const data = await apiGet<{ providers: ProviderConfig[]; active: string }>('/api/providers')
      usingMock.value = false
      applyList(data.providers ?? [], data.active ?? '')
    } catch (e) {
      usingMock.value = true
      lastError.value = String(e)
      applyList(MOCK_PROVIDERS, 'openai')
    } finally {
      loading.value = false
    }
  }

  function selectModel(providerId: string, model: string) {
    currentProviderId.value = providerId; currentModel.value = model
    localStorage.setItem('coomi.providerId', providerId)
    localStorage.setItem('coomi.model', model)
  }
  function setPermissionMode(mode: PermissionMode) {
    permissionMode.value = mode
    localStorage.setItem('coomi.permissionMode', mode)
  }
  function cyclePermissionMode(): PermissionMode {
    const order: PermissionMode[] = ['ask', 'auto', 'full']
    const idx = order.indexOf(permissionMode.value)
    permissionMode.value = order[(idx + 1) % order.length]
    return permissionMode.value
  }
  function togglePlanMode() { planMode.value = !planMode.value }

  /** 新增/更新 Provider。空 apiKey 表示沿用旧 key（后端语义）。 */
  async function upsertProvider(input: ProviderInput): Promise<boolean> {
    if (usingMock.value) {
      // 浏览器兜底：仅本地更新，不落盘
      const masked = input.apiKey ? '****' + input.apiKey.slice(-4) : '****'
      const existing = providers.value.find(p => p.id === input.id)
      if (existing) { existing.name = input.name; existing.apiKeyMasked = masked; existing.models = input.models; existing.baseUrl = input.baseUrl; existing.type = input.type; existing.toolProtocol = input.toolProtocol; existing.contextWindow = input.contextWindow }
      else { providers.value.push({ id: input.id, name: input.name, apiKeyMasked: masked, hasKey: !!input.apiKey, models: input.models, baseUrl: input.baseUrl, type: input.type, toolProtocol: input.toolProtocol, contextWindow: input.contextWindow }) }
      return true
    }
    try {
      await apiSend('/api/providers', 'POST', {
        id: input.id,
        name: input.name,
        apiKey: input.apiKey,
        models: input.models,
        model: input.models[0],
        baseUrl: input.baseUrl,
        type: input.type,
        toolProtocol: input.toolProtocol,
        contextWindow: input.contextWindow,
        fastModel: input.fastModel,
        supportsWebSearch: input.supportsWebSearch,
        activate: input.activate,
      })
      await fetchProviders()
      return true
    } catch (e) {
      lastError.value = String(e)
      return false
    }
  }

  async function deleteProvider(id: string): Promise<boolean> {
    if (usingMock.value) {
      providers.value = providers.value.filter(p => p.id !== id)
      return true
    }
    try {
      await apiSend(`/api/providers/${encodeURIComponent(id)}`, 'DELETE')
      await fetchProviders()
      return true
    } catch (e) {
      lastError.value = String(e)
      return false
    }
  }

  async function activateProvider(id: string): Promise<boolean> {
    if (usingMock.value) { activeId.value = id; return true }
    try {
      await apiSend(`/api/providers/${encodeURIComponent(id)}/activate`, 'POST')
      await fetchProviders()
      return true
    } catch (e) {
      lastError.value = String(e)
      return false
    }
  }

  async function copyProvider(id: string): Promise<string | null> {
    try {
      const result = await apiSend<{ id: string }>(`/api/providers/${encodeURIComponent(id)}/copy`, 'POST')
      await fetchProviders()
      return result.id
    } catch (e) {
      lastError.value = String(e)
      return null
    }
  }

  async function revealProviderKey(id: string): Promise<string | null> {
    try {
      const result = await apiSend<{ apiKey: string }>(`/api/providers/${encodeURIComponent(id)}/reveal`, 'POST')
      return result.apiKey
    } catch (e) {
      lastError.value = String(e)
      return null
    }
  }

  async function discoverModels(id: string): Promise<string[] | null> {
    try {
      const result = await apiSend<{ models: string[] }>(`/api/providers/${encodeURIComponent(id)}/discover-models`, 'POST')
      await fetchProviders()
      return result.models
    } catch (e) {
      lastError.value = String(e)
      return null
    }
  }

  return {
    permissionMode, planMode, providers, activeId, loading, usingMock, lastError,
    currentProviderId, currentModel, currentProvider,
    fetchProviders, selectModel, setPermissionMode, cyclePermissionMode, togglePlanMode,
    upsertProvider, deleteProvider, activateProvider, copyProvider, revealProviderKey, discoverModels,
  }
})
