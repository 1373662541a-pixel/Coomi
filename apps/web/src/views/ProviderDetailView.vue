<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import PageHead from '@/components/PageHead.vue'
import CoomiIcon from '@/components/CoomiIcon.vue'
import ModelPickerSheet from '@/components/ModelPickerSheet.vue'
import {
  BUILTIN_PROVIDER_PRESETS,
  useConfigStore,
  type ProviderConfig,
  type ProviderProtocol,
} from '@/stores/config'

type Tab = 'config' | 'models'

interface ProviderDraft {
  id: string
  name: string
  apiKey: string
  models: string[]
  modelContextWindows: Record<string, number>
  baseUrl: string
  protocol: ProviderProtocol
  contextWindow: number
  supportsWebSearch: boolean
  supportsVision: boolean
}

const route = useRoute()
const router = useRouter()
const config = useConfigStore()

const tab = ref<Tab>('config')
const draft = ref<ProviderDraft | null>(null)
const original = ref<ProviderDraft | null>(null)
const isNew = computed(() => route.name === 'provider-new')
const idLocked = ref(false)
const saving = ref(false)
const discovering = ref(false)
const showingKey = ref(false)
const showPicker = ref(false)
const candidates = ref<string[]>([])
const manualModel = ref('')
const message = ref('')
const error = ref('')
const pendingDelete = ref(false)
const pendingClear = ref(false)
const pendingBack = ref(false)
const customContextWindow = ref(64)
const CONTEXT_WINDOW_PRESETS = [128000, 256000, 512000, 1048576]
const SAVE_FEEDBACK_MIN_MS = 200

const providerId = computed(() => {
  const value = route.params.id
  return typeof value === 'string' ? decodeURIComponent(value) : ''
})
const provider = computed(() => config.mergedProviders.find(item => item.id === providerId.value) ?? null)
const isBuiltin = computed(() => provider.value?.builtin ?? false)
const isCurrent = computed(() => provider.value?.id === config.activeId)
const canSave = computed(() => Boolean(
  draft.value?.id.trim() && draft.value?.name.trim() && draft.value?.baseUrl.trim(),
))
const dirty = computed(() => JSON.stringify(draft.value) !== JSON.stringify(original.value))

function waitForSaveFeedback(startedAt: number) {
  const remaining = SAVE_FEEDBACK_MIN_MS - (Date.now() - startedAt)
  return remaining > 0 ? new Promise<void>(resolve => setTimeout(resolve, remaining)) : Promise.resolve()
}

const protocols: { value: ProviderProtocol; label: string }[] = [
  { value: 'openai_compatible', label: 'OpenAI Compatible' },
  { value: 'openai_responses', label: 'OpenAI Responses' },
  { value: 'anthropic_messages', label: 'Anthropic Messages' },
  { value: 'gemini_native', label: 'Google Gemini (Native)' },
]

function normalizeProtocol(value?: string): ProviderProtocol {
  if (value === 'openai_responses' || value === 'responses') return 'openai_responses'
  if (value === 'anthropic_messages' || value === 'anthropic') return 'anthropic_messages'
  if (value === 'gemini_native' || value === 'gemini') return 'gemini_native'
  return 'openai_compatible'
}

function emptyDraft(id = '', name = ''): ProviderDraft {
  const preset = BUILTIN_PROVIDER_PRESETS.find(item => item.id === id)
  return {
    id,
    name: name || preset?.name || '',
    apiKey: '',
    models: [],
    modelContextWindows: {},
    baseUrl: preset?.baseUrl || '',
    protocol: preset?.protocol || 'openai_compatible',
    contextWindow: 256000,
    supportsWebSearch: false,
    supportsVision: false,
  }
}

function draftFromProvider(value: ProviderConfig | null): ProviderDraft {
  if (!value) return emptyDraft(isNew.value ? '' : providerId.value)
  return {
    id: value.id,
    name: value.name,
    apiKey: '',
    models: [...value.models],
    modelContextWindows: { ...(value.modelContextWindows ?? {}) },
    baseUrl: value.baseUrl || '',
    protocol: normalizeProtocol(value.toolProtocol || value.type),
    contextWindow: value.contextWindow || 256000,
    supportsWebSearch: !!value.supportsWebSearch,
    supportsVision: !!value.supportsVision,
  }
}

function clone(value: ProviderDraft): ProviderDraft {
  return {
    ...value,
    models: [...value.models],
    modelContextWindows: { ...value.modelContextWindows },
  }
}

function normalizeModels(models: string[]): string[] {
  return Array.from(new Set(models.map(model => model.trim()).filter(Boolean)))
}

function formatContextWindow(value: number): string {
  return value === 1048576 ? '1024k' : `${Math.round(value / 1000)}k`
}

function modelContextSelection(model: string): string {
  const value = draft.value?.modelContextWindows[model]
  if (!value) return 'default'
  return CONTEXT_WINDOW_PRESETS.includes(value) ? String(value) : 'custom'
}

function modelCustomContextWindow(model: string): number {
  return draft.value?.modelContextWindows[model]
    ?? draft.value?.contextWindow
    ?? 256000
}

function updateModelContextWindow(model: string, event: Event) {
  if (!draft.value) return
  const selection = (event.target as HTMLSelectElement).value
  const windows = { ...draft.value.modelContextWindows }
  if (selection === 'default') {
    delete windows[model]
  } else if (selection === 'custom') {
    const value = Math.max(32, Math.min(1048, Math.round(modelCustomContextWindow(model) / 1000))) * 1000
    windows[model] = value
  } else {
    windows[model] = Number(selection)
  }
  draft.value.modelContextWindows = windows
}

function updateModelCustomContextWindow(model: string, event: Event) {
  if (!draft.value) return
  const value = Math.max(32, Math.min(1048, Math.round(Number((event.target as HTMLInputElement).value) || 64))) * 1000
  draft.value.modelContextWindows = { ...draft.value.modelContextWindows, [model]: value }
}

function loadDraft() {
  const next = draftFromProvider(provider.value)
  draft.value = next
  original.value = clone(next)
  idLocked.value = !isNew.value
  if (!isNew.value && !provider.value) router.replace('/providers')
}

onMounted(async () => {
  await config.fetchProviders()
  loadDraft()
})

watch(() => route.fullPath, () => {
  tab.value = 'config'
  message.value = ''
  error.value = ''
  loadDraft()
})

function back() {
  if (dirty.value) {
    pendingBack.value = true
    return
  }
  router.push('/providers')
}

function discardAndBack() {
  pendingBack.value = false
  router.push('/providers')
}

async function saveConfig() {
  if (!draft.value || !canSave.value) return false
  const value = draft.value
  value.models = normalizeModels(value.models)
  const startedAt = Date.now()
  saving.value = true
  error.value = ''
  const ok = await config.upsertProvider({
    id: value.id.trim(),
    name: value.name.trim(),
    apiKey: value.apiKey.trim(),
    models: [...value.models],
    modelContextWindows: { ...value.modelContextWindows },
    baseUrl: value.baseUrl.trim(),
    type: value.protocol,
    toolProtocol: value.protocol,
    contextWindow: value.contextWindow === 0 ? Math.max(32, Math.min(1048, Math.round(customContextWindow.value || 64))) * 1000 : value.contextWindow,
    supportsWebSearch: value.supportsWebSearch,
    supportsVision: value.supportsVision,
    activate: false,
  })
  await waitForSaveFeedback(startedAt)
  saving.value = false
  if (!ok) {
    error.value = config.lastError || '保存失败'
    return false
  }
  idLocked.value = true
  original.value = clone(value)
  message.value = '配置已保存'
  if (isNew.value) {
    await router.replace(`/providers/${encodeURIComponent(value.id)}`)
  }
  return true
}

async function saveModels() {
  if (!draft.value) return false
  if (!(await saveConfig())) return false
  const value = draft.value
  const startedAt = Date.now()
  saving.value = true
  error.value = ''
  const ok = await config.upsertProvider({
    id: value.id,
    name: value.name,
    apiKey: '',
    models: [...value.models],
    modelContextWindows: { ...value.modelContextWindows },
    baseUrl: value.baseUrl,
    type: value.protocol,
    toolProtocol: value.protocol,
    contextWindow: value.contextWindow === 0 ? Math.max(32, Math.min(1048, Math.round(customContextWindow.value || 64))) * 1000 : value.contextWindow,
    supportsWebSearch: value.supportsWebSearch,
    supportsVision: value.supportsVision,
    activate: false,
  })
  await waitForSaveFeedback(startedAt)
  saving.value = false
  if (!ok) {
    error.value = config.lastError || '模型保存失败'
    return false
  }
  original.value = clone(value)
  message.value = '模型列表已保存'
  return true
}

async function revealKey() {
  if (!draft.value || isNew.value || showingKey.value) {
    showingKey.value = !showingKey.value
    return
  }
  const value = await config.revealProviderKey(draft.value.id)
  if (value !== null) {
    draft.value.apiKey = value
    if (original.value) original.value.apiKey = value
    showingKey.value = true
  } else {
    error.value = config.lastError || '无法读取 API Key'
  }
}

async function discover() {
  if (!draft.value) return
  if (!draft.value.apiKey.trim() && !provider.value?.hasKey) {
    error.value = '请先填写 API Key 后再获取模型'
    return
  }
  if (!draft.value.baseUrl.trim()) {
    error.value = '请先填写 API Base URL 后再获取模型'
    return
  }
  if (!(await saveConfig())) return
  discovering.value = true
  error.value = ''
  const models = await config.discoverModels(draft.value.id)
  discovering.value = false
  if (models === null) {
    error.value = config.lastError || '模型获取失败'
    return
  }
  candidates.value = Array.from(new Set(models.map(model => model.trim()).filter(Boolean)))
  showPicker.value = true
}

function addManualModel() {
  if (!draft.value) return
  const value = manualModel.value.trim()
  if (!value || draft.value.models.includes(value)) return
  draft.value.models.push(value)
  manualModel.value = ''
}

function removeModel(model: string) {
  if (!draft.value) return
  draft.value.models = draft.value.models.filter(item => item !== model)
  const windows = { ...draft.value.modelContextWindows }
  delete windows[model]
  draft.value.modelContextWindows = windows
}

async function clearBuiltin() {
  if (!draft.value || !isBuiltin.value) return
  if (isCurrent.value) {
    error.value = '请先切换到其他供应商，再清空当前供应商'
    pendingClear.value = false
    return
  }
  pendingClear.value = false
  if (!(await config.clearProvider(draft.value.id))) {
    error.value = config.lastError || '清空失败'
    return
  }
  router.push('/providers')
}

async function deleteCustom() {
  if (!draft.value || isBuiltin.value) return
  if (isCurrent.value) {
    error.value = '请先切换到其他供应商，再删除当前供应商'
    pendingDelete.value = false
    return
  }
  pendingDelete.value = false
  if (!(await config.deleteProvider(draft.value.id))) {
    error.value = config.lastError || '删除失败'
    return
  }
  router.push('/providers')
}

function onPickerConfirm(models: string[]) {
  const next = normalizeModels(models)
  if (draft.value) draft.value.models = next
  if (draft.value) {
    const windows = { ...draft.value.modelContextWindows }
    for (const model of Object.keys(windows)) {
      if (!next.includes(model)) delete windows[model]
    }
    draft.value.modelContextWindows = windows
  }
  showPicker.value = false
}

function openModelConfig() {
  tab.value = 'models'
}
</script>

<template>
  <div class="page">
    <PageHead :title="draft?.name || (isNew ? '新建供应商' : '供应商详情')" @back="back">
      <template #right>
        <button v-if="tab === 'config'" class="head-save" :disabled="!canSave || saving" @click="saveConfig">保存</button>
      </template>
    </PageHead>

    <main v-if="draft" class="body">
      <div class="tabs" role="tablist">
        <button :class="{ on: tab === 'config' }" @click="tab = 'config'"><CoomiIcon name="settings" :size="16" />配置</button>
        <button :class="{ on: tab === 'models' }" @click="tab = 'models'"><CoomiIcon name="cpu" :size="16" />模型<span class="count">{{ draft.models.length }}</span></button>
      </div>

      <p v-if="message" class="notice ok">{{ message }}</p>
      <p v-if="error" class="notice err">{{ error }}</p>

      <form v-if="tab === 'config'" class="form" @submit.prevent="saveConfig">
        <label class="field">
          <span>名称</span>
          <input v-model="draft.name" class="input" placeholder="例如 OpenAI" />
        </label>
        <label class="field">
          <span>供应商 ID</span>
          <input v-model="draft.id" class="input" :readonly="idLocked || (isBuiltin && !isNew)" placeholder="例如 my-provider" autocapitalize="off" />
        </label>
        <label class="field">
          <span>API Key</span>
          <span class="input-action">
            <input v-model="draft.apiKey" class="input" :type="showingKey ? 'text' : 'password'" :placeholder="isNew ? '输入 API Key' : '留空以保留原 Key'" autocomplete="off" autocapitalize="off" />
            <button type="button" aria-label="查看 API Key" @click="revealKey"><CoomiIcon :name="showingKey ? 'eye' : 'eye'" :size="17" /></button>
          </span>
        </label>
        <label class="field">
          <span>API Base URL</span>
          <input v-model="draft.baseUrl" class="input" placeholder="https://api.openai.com/v1" inputmode="url" autocapitalize="off" />
        </label>
        <label class="field">
          <span>协议</span>
          <select v-model="draft.protocol" class="input">
            <option v-for="item in protocols" :key="item.value" :value="item.value">{{ item.label }}</option>
          </select>
        </label>
        <label class="field">
          <span>默认上下文窗口</span>
          <select v-model.number="draft.contextWindow" class="input">
            <option :value="128000">128k</option>
            <option :value="256000">256k</option>
            <option :value="512000">512k</option>
            <option :value="1048576">1024k</option>
            <option :value="0">自定义</option>
          </select>
          <input v-if="draft.contextWindow === 0" v-model.number="customContextWindow" class="input" type="number" min="32" max="1048" placeholder="单位：k" />
        </label>
        <label class="toggle"><input v-model="draft.supportsWebSearch" type="checkbox" /><span>使用供应商原生 Web Search</span></label>
        <label class="toggle"><input v-model="draft.supportsVision" type="checkbox" /><span>支持图像理解</span></label>
        <button class="btn btn-primary wide" type="submit" :disabled="!canSave || saving">{{ saving ? '保存中...' : '保存配置' }}</button>
        <button
          v-if="message && draft.models.length === 0"
          type="button"
          class="model-prompt"
          @click="openModelConfig"
        >
          请前往配置可用模型
          <CoomiIcon name="chevronRight" :size="15" />
        </button>
      </form>

      <section v-else class="models-panel">
        <div class="model-tools">
          <label class="input-action add-model">
            <input v-model="manualModel" class="input" placeholder="输入模型 ID" autocomplete="off" autocapitalize="off" @keyup.enter="addManualModel" />
            <button type="button" aria-label="添加模型" @click="addManualModel"><CoomiIcon name="plus" :size="17" /></button>
          </label>
          <button class="btn btn-soft discover" :disabled="discovering || saving" @click="discover"><CoomiIcon name="refresh" :size="16" />{{ discovering ? '获取中...' : '在线获取' }}</button>
        </div>
        <p class="subnote">每个模型可单独设置上下文窗口，选择“默认”时继承供应商配置。</p>
        <div class="model-list">
          <div v-for="model in draft.models" :key="model" class="model-item">
            <code>{{ model }}</code>
            <select
              class="model-window"
              :value="modelContextSelection(model)"
              :aria-label="`${model} 上下文窗口`"
              @change="updateModelContextWindow(model, $event)"
            >
              <option value="default">默认 {{ formatContextWindow(draft.contextWindow) }}</option>
              <option v-for="value in CONTEXT_WINDOW_PRESETS" :key="value" :value="value">{{ formatContextWindow(value) }}</option>
              <option value="custom">自定义</option>
            </select>
            <input
              v-if="modelContextSelection(model) === 'custom'"
              class="model-window-custom"
              type="number"
              min="32"
              max="1048"
              :value="Math.round(modelCustomContextWindow(model) / 1000)"
              aria-label="自定义上下文窗口（千 token）"
              @change="updateModelCustomContextWindow(model, $event)"
            />
            <div class="model-actions">
              <button class="icon-btn danger-icon" aria-label="移除模型" @click="removeModel(model)"><CoomiIcon name="close" :size="15" /></button>
            </div>
          </div>
          <p v-if="draft.models.length === 0" class="empty">还没有模型。可以手动添加，或在线获取。</p>
        </div>
        <button class="btn btn-primary wide" :disabled="saving || !dirty" @click="saveModels">{{ saving ? '保存中...' : '保存模型列表' }}</button>
      </section>

      <div class="danger-area">
        <button v-if="isBuiltin" class="danger-link" :disabled="isCurrent" @click="pendingClear = true">清空配置</button>
        <button v-else class="danger-link" :disabled="isCurrent" @click="pendingDelete = true">删除供应商</button>
      </div>
    </main>

    <ModelPickerSheet v-if="showPicker" :candidates="candidates" :selected="draft?.models || []" @cancel="showPicker = false" @confirm="onPickerConfirm" />

    <div v-if="pendingBack || pendingClear || pendingDelete" class="mask" @click.self="pendingBack = pendingClear = pendingDelete = false">
      <section class="confirm-sheet">
        <div class="grip" />
        <h2>{{ pendingBack ? '放弃未保存的修改？' : pendingClear ? '清空供应商配置？' : '删除供应商？' }}</h2>
        <p>{{ pendingBack ? '当前修改还没有保存，离开后会丢失。' : pendingClear ? '只会删除已保存的配置，内置供应商仍会保留在列表中。' : '该供应商的 API Key 和模型配置也会一并删除。' }}</p>
        <div class="confirm-actions">
          <button class="btn btn-ghost" @click="pendingBack = pendingClear = pendingDelete = false">取消</button>
          <button v-if="pendingBack" class="btn btn-danger" @click="discardAndBack">放弃修改</button>
          <button v-else class="btn btn-danger" @click="pendingClear ? clearBuiltin() : deleteCustom()">确认</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.page { display: flex; flex-direction: column; height: 100%; background: var(--page); }
.body { flex: 1; min-height: 0; overflow-y: auto; padding: 12px 12px calc(var(--safe-bottom) + 24px); }
.head-save { min-height: 34px; padding: 0 11px; border-radius: var(--r-sm); color: var(--blue); font-size: 13px; font-weight: 650; }
.head-save:disabled { color: var(--text-3); }
.tabs { display: flex; gap: 3px; padding: 3px; margin-bottom: 12px; border-radius: var(--r-pill); background: var(--fill-strong); }
.tabs button { display: flex; align-items: center; justify-content: center; gap: 6px; flex: 1; min-height: 36px; border-radius: var(--r-pill); color: var(--text-3); font-size: 13.5px; font-weight: 600; }
.tabs button.on { background: var(--bg); color: var(--blue); box-shadow: var(--shadow-1); }
.count { min-width: 18px; height: 18px; padding: 0 5px; border-radius: var(--r-pill); background: var(--fill); font-size: 11px; line-height: 18px; text-align: center; }
.notice { margin: 0 0 10px; padding: 9px 11px; border-radius: var(--r-md); font-size: 12.5px; line-height: 1.55; }
.notice.ok { color: var(--ok); background: var(--ok-soft); }
.notice.err { color: var(--danger); background: var(--danger-soft); }
.form { display: flex; flex-direction: column; gap: 12px; }
.field { display: flex; flex-direction: column; gap: 5px; }
.field > span:first-child { padding-left: 3px; color: var(--text-2); font-size: 12.5px; }
.input { width: 100%; min-height: 44px; padding: 0 12px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--bg); color: var(--text); font-size: 14px; }
.input::placeholder { color: var(--text-3); }
.input:focus { border-color: var(--blue-border); outline: none; }
.input[readonly] { background: var(--fill); color: var(--text-2); }
.input-action { display: flex; align-items: stretch; gap: 7px; }
.input-action .input { flex: 1; min-width: 0; }
.input-action > button { display: grid; place-items: center; flex: 0 0 45px; border-radius: var(--r-sm); background: var(--fill-strong); color: var(--blue); }
.toggle { display: flex; align-items: center; gap: 9px; min-height: 38px; padding: 0 3px; color: var(--text-2); font-size: 13px; }
.toggle input { width: 18px; height: 18px; accent-color: var(--blue); }
.wide { width: 100%; margin-top: 3px; }
.model-prompt { display: flex; align-items: center; justify-content: center; gap: 4px; width: 100%; min-height: 38px; margin-top: 4px; color: var(--blue); font-size: 13px; }
.model-tools { display: flex; gap: 8px; }
.add-model { flex: 1; min-width: 0; }
.discover { flex-shrink: 0; min-height: 44px; padding: 0 12px; }
.subnote { margin: 8px 3px 12px; color: var(--text-3); font-size: 12px; line-height: 1.55; }
.model-list { overflow: hidden; border-radius: var(--r-card); background: var(--bg); box-shadow: var(--shadow-1); }
.model-item { display: flex; align-items: center; gap: 8px; min-height: 52px; padding: 8px 10px 8px 13px; }
.model-item + .model-item { border-top: 1px solid var(--border); }
.model-item code { flex: 1; min-width: 0; overflow-wrap: anywhere; font-family: var(--font-mono); font-size: 12.5px; color: var(--text); }
.model-window { flex: 0 0 88px; min-height: 30px; padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg); color: var(--text-2); font-size: 11px; }
.model-window-custom { flex: 0 0 58px; min-height: 30px; padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--bg); color: var(--text-2); font-size: 11px; }
.model-actions { display: flex; align-items: center; gap: 5px; flex-shrink: 0; }
.danger-icon { width: 30px; height: 30px; color: var(--danger); }
.empty { padding: 20px 12px; text-align: center; color: var(--text-3); font-size: 13px; }
.danger-area { display: flex; justify-content: center; margin-top: 24px; }
.danger-link { min-height: 36px; padding: 0 12px; color: var(--danger); font-size: 13px; }
.danger-link:disabled { color: var(--text-3); }
.mask { position: fixed; inset: 0; z-index: 80; display: flex; align-items: flex-end; background: rgba(17, 22, 31, .42); }
.confirm-sheet { width: 100%; padding: 8px 16px calc(var(--safe-bottom) + 16px); border-radius: 22px 22px 0 0; background: var(--bg); box-shadow: var(--shadow-sheet); }
.grip { width: 38px; height: 4px; margin: 3px auto 14px; border-radius: 2px; background: var(--border-strong); }
.confirm-sheet h2 { font-size: 16px; color: var(--text); }
.confirm-sheet p { margin: 7px 0 0; color: var(--text-2); font-size: 13px; line-height: 1.6; }
.confirm-actions { display: flex; gap: 9px; margin-top: 16px; }
.confirm-actions .btn { flex: 1; }
.btn-danger { background: var(--danger); color: #fff; }
</style>
