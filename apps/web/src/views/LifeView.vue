<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { apiGet, apiSend } from '@/bridge/http'
import { goBack } from '@/bridge/navigation'
import { useConfigStore } from '@/stores/config'
import { useSessionStore } from '@/stores/session'
import PageHead from '@/components/PageHead.vue'
import CoomiIcon from '@/components/CoomiIcon.vue'

interface LifeProfile {
  name: string
  address: string
  paused: boolean
  emotion: string
  attention: string
  bond: number
  needs: Record<string, number>
  personality?: Record<string, string>
  memory_count: number
  updated_at_ms: number
}

interface LifeStatus {
  installed: boolean
  runtime_ready: boolean
  profile: LifeProfile | null
  dependencies: string[]
  upstream_commit: string
  background_heartbeat: boolean
}

const router = useRouter()
const config = useConfigStore()
const session = useSessionStore()
const status = ref<LifeStatus | null>(null)
const busy = ref('')
const message = ref('')
const error = ref('')
const name = ref('Coomi Life')
const address = ref('你')
const preset = ref('balanced')
const memoryQuery = ref('')
const memories = ref<string[]>([])
const exportedPath = ref('')

const profile = computed(() => status.value?.profile ?? null)
const bondPercent = computed(() => Math.round((profile.value?.bond ?? 0) * 100))

async function refresh() {
  error.value = ''
  try {
    status.value = await apiGet<LifeStatus>('/api/cognitive/status')
    if (status.value.profile) {
      name.value = status.value.profile.name
      address.value = status.value.profile.address
    } else if (config.digitalLifeEnabled) {
      config.setDigitalLifeEnabled(false)
    }
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  }
}

async function run(label: string, operation: () => Promise<unknown>, success: string) {
  if (busy.value) return
  busy.value = label
  error.value = ''
  message.value = ''
  try {
    await operation()
    message.value = success
    await refresh()
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = ''
  }
}

function install() {
  return run('install', () => apiSend('/api/cognitive/install', 'POST'), '扩展已安装')
}

function uninstall() {
  return run('uninstall', () => apiSend('/api/cognitive/install', 'DELETE'), '扩展代码已卸载')
}

function bootstrap() {
  return run('bootstrap', () => apiSend('/api/cognitive/bootstrap', 'POST', {
    profile_id: 'primary', name: name.value, address: address.value,
  }), '觉醒完成')
}

function configure() {
  return run('configure', () => apiSend('/api/cognitive/configure', 'POST', {
    profile_id: 'primary', name: name.value, address: address.value, preset: preset.value,
  }), '配置已保存')
}

function togglePause() {
  return run('pause', () => apiSend('/api/cognitive/pause', 'POST', {
    profile_id: 'primary', paused: !profile.value?.paused,
  }), profile.value?.paused ? '已恢复' : '已暂停')
}

function openRuntime() {
  router.push('/runtime')
}

function toggleEnabled() {
  if (!profile.value) return
  const enabled = !config.digitalLifeEnabled
  config.setDigitalLifeEnabled(enabled)
  session.setSessionMode(enabled ? 'life' : 'agent')
}

async function recall() {
  if (busy.value) return
  busy.value = 'memory'
  error.value = ''
  try {
    memories.value = await apiSend<string[]>('/api/cognitive/memory', 'POST', {
      profile_id: 'primary', query: memoryQuery.value, limit: 8,
    })
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = ''
  }
}

async function exportProfile() {
  if (busy.value) return
  busy.value = 'export'
  error.value = ''
  try {
    const result = await apiSend<{ path: string }>('/api/cognitive/export', 'POST', { profile_id: 'primary' })
    exportedPath.value = result.path
    message.value = '导出完成'
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    busy.value = ''
  }
}

function resetProfile() {
  return run('reset', () => apiSend('/api/cognitive/reset', 'POST', { profile_id: 'primary' }), '状态和记忆已重置')
}

function deleteProfile() {
  return run('delete', () => apiSend('/api/cognitive/delete', 'POST', { profile_id: 'primary' }), '数据已彻底删除')
}

onMounted(() => {
  config.syncDigitalLifeEnabled()
  void refresh()
})
</script>

<template>
  <div class="page">
    <PageHead title="数字生命体（实验）" @back="goBack(router, 'dashboard')" />
    <main class="body">
      <div v-if="error || message" class="notice" :class="{ error: !!error }">{{ error || message }}</div>

      <p class="sec-label">模式</p>
      <section class="group enable-group">
        <button :disabled="!profile" @click="toggleEnabled">
          <span class="life-mark"><CoomiIcon name="lifeRings" :size="22" /></span>
          <span><strong>启用数字生命</strong><small>{{ profile ? '开启后对话将由数字生命模式接管' : '完成安装和觉醒后可开启' }}</small></span>
          <i class="switch" :class="{ on: config.digitalLifeEnabled }" />
        </button>
      </section>

      <p class="sec-label">扩展</p>
      <section class="group status-group">
        <div class="status-row"><span>ProotLinux</span><strong :class="{ ok: status?.runtime_ready }">{{ status?.runtime_ready ? '可用' : '未就绪' }}</strong></div>
        <div class="status-row"><span>Coomi Life</span><strong :class="{ ok: status?.installed }">{{ status?.installed ? '已安装' : '未安装' }}</strong></div>
        <div class="actions">
          <button v-if="!status?.installed && !status?.runtime_ready" class="secondary" :disabled="!!busy" @click="openRuntime"><CoomiIcon name="download" :size="16" />先安装 ProotLinux</button>
          <button v-else-if="!status?.installed" class="primary" :disabled="!!busy" @click="install"><CoomiIcon name="download" :size="16" />安装</button>
          <button v-else class="secondary" :disabled="!!busy" @click="uninstall"><CoomiIcon name="trash" :size="16" />卸载扩展</button>
        </div>
      </section>

      <template v-if="status?.installed">
        <p class="sec-label">身份</p>
        <section class="group form-group">
          <label><span>名称</span><input v-model="name" maxlength="48" /></label>
          <label><span>称呼</span><input v-model="address" maxlength="48" /></label>
          <label><span>人格预设</span><select v-model="preset"><option value="balanced">均衡</option><option value="warm">温暖</option><option value="direct">直接</option></select></label>
          <div class="actions">
            <button v-if="!profile" class="primary" :disabled="!!busy" @click="bootstrap">觉醒</button>
            <button v-else class="primary" :disabled="!!busy" @click="configure">保存</button>
          </div>
        </section>

        <template v-if="profile">
          <p class="sec-label">状态</p>
          <section class="group metrics">
            <div><span>情绪</span><strong>{{ profile.emotion }}</strong></div>
            <div><span>关注</span><strong>{{ profile.attention }}</strong></div>
            <div><span>关系</span><strong>{{ bondPercent }}%</strong></div>
            <div><span>记忆</span><strong>{{ profile.memory_count }}</strong></div>
            <div v-for="(value, key) in profile.needs" :key="key"><span>{{ key }}</span><strong>{{ Math.round(value * 100) }}%</strong></div>
          </section>
          <div class="actions standalone">
            <button class="secondary" :disabled="!!busy" @click="togglePause"><CoomiIcon :name="profile.paused ? 'play' : 'pause'" :size="16" />{{ profile.paused ? '恢复' : '暂停' }}</button>
            <button class="secondary" :disabled="!!busy" @click="exportProfile"><CoomiIcon name="download" :size="16" />导出</button>
          </div>
          <p v-if="exportedPath" class="path">{{ exportedPath }}</p>

          <p class="sec-label">记忆</p>
          <section class="group memory-group">
            <div class="search"><input v-model="memoryQuery" placeholder="检索记忆" @keyup.enter="recall" /><button aria-label="检索" :disabled="!!busy" @click="recall"><CoomiIcon name="search" :size="17" /></button></div>
            <p v-if="memories.length === 0" class="empty">暂无匹配记忆</p>
            <p v-for="(item, index) in memories" :key="index" class="memory">{{ item }}</p>
          </section>

          <p class="sec-label danger-label">数据</p>
          <section class="group danger-actions">
            <button :disabled="!!busy" @click="resetProfile">重置状态和记忆</button>
            <button class="danger" :disabled="!!busy" @click="deleteProfile">彻底删除</button>
          </section>
        </template>
      </template>
    </main>
  </div>
</template>

<style scoped>
.page { display: flex; flex-direction: column; height: 100%; background: var(--page); }
.body { flex: 1; overflow-y: auto; padding: 14px 12px calc(var(--safe-bottom) + 24px); }
.sec-label { margin: 16px 0 0; }
.sec-label:first-of-type { margin-top: 2px; }
.group { overflow: hidden; border-radius: var(--r-card); background: var(--bg); box-shadow: var(--shadow-1); }
.notice { margin-bottom: 10px; padding: 9px 11px; border-radius: 6px; background: var(--blue-soft); color: var(--blue); font-size: 12.5px; }
.notice.error { background: color-mix(in srgb, var(--danger) 10%, var(--bg)); color: var(--danger); }
.enable-group button { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 62px; padding: 10px 13px; text-align: left; }
.enable-group button > span { display: flex; flex: 1; min-width: 0; flex-direction: column; }
.enable-group button > .life-mark { display: grid; place-items: center; flex: none; width: 36px; height: 36px; border-radius: 50%; color: var(--blue); background: var(--blue-soft); }
.enable-group strong { color: var(--text); font-size: 14px; font-weight: 600; }
.enable-group small { margin-top: 2px; color: var(--text-3); font-size: 12px; }
.switch { position: relative; flex: none; width: 42px; height: 24px; border-radius: 12px; background: var(--border-strong); transition: background .2s; }
.switch::after { content: ''; position: absolute; top: 2px; left: 2px; width: 20px; height: 20px; border-radius: 50%; background: #fff; box-shadow: var(--shadow-1); transition: transform .2s; }
.switch.on { background: var(--blue); }
.switch.on::after { transform: translateX(18px); }
.status-row, .metrics > div { display: flex; align-items: center; justify-content: space-between; min-height: 48px; padding: 0 13px; border-bottom: 1px solid var(--border); font-size: 13px; }
.status-row strong, .metrics strong { color: var(--text-2); font-variant-numeric: tabular-nums; }
.status-row strong.ok { color: var(--ok); }
.actions { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 12px; }
.actions.standalone { padding: 10px 0 0; }
.actions button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; min-height: 36px; padding: 0 13px; border-radius: 6px; font-size: 13px; font-weight: 600; }
.primary { background: var(--blue); color: #fff; }
.secondary { background: var(--fill-strong); color: var(--text-2); }
button:disabled { opacity: .45; }
.form-group label { display: grid; grid-template-columns: 82px minmax(0, 1fr); align-items: center; gap: 10px; min-height: 56px; padding: 8px 13px; border-bottom: 1px solid var(--border); font-size: 13px; }
.form-group input, .form-group select, .search input { min-width: 0; height: 38px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--page); color: var(--text); }
.metrics { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); }
.metrics > div:nth-child(odd) { border-right: 1px solid var(--border); }
.path { overflow-wrap: anywhere; margin: 7px 2px 0; color: var(--text-3); font-size: 11px; }
.search { display: flex; gap: 8px; padding: 10px 12px; border-bottom: 1px solid var(--border); }
.search input { flex: 1; }
.search button { display: grid; place-items: center; width: 38px; height: 38px; border-radius: 6px; background: var(--fill-strong); color: var(--text-2); }
.empty, .memory { margin: 0; padding: 11px 13px; color: var(--text-3); font-size: 12.5px; line-height: 1.55; }
.memory + .memory { border-top: 1px solid var(--border); }
.memory { color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
.danger-label { color: var(--danger); }
.danger-actions { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); padding: 8px; gap: 8px; }
.danger-actions button { min-height: 38px; border-radius: 6px; background: var(--fill); color: var(--text-2); font-size: 13px; }
.danger-actions button.danger { background: color-mix(in srgb, var(--danger) 10%, var(--bg)); color: var(--danger); }
</style>
