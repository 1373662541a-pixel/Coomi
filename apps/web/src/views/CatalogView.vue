<script setup lang="ts">
/**
 * SKILL / MCP 管理（控制台入口）。
 * 数据来自引擎 /api/catalog；安装走 /api/catalog/{mcp,skills}/install。
 */
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import PageHead from '@/components/PageHead.vue'
import CoomiIcon from '@/components/CoomiIcon.vue'
import { authedFetch } from '@/bridge/http'

const router = useRouter()

type Tab = 'mcp' | 'skills'
const tab = ref<Tab>('mcp')

interface RequiredParam { key: string; label: string; secret?: boolean }
interface McpItem {
  id: string; name: string; description: string; transport: string
  required_parameters: RequiredParam[]; installed: boolean; enabled: boolean
}
interface SkillItem { id: string; name: string; description: string; repository: string; installed: boolean }

const mcp = ref<McpItem[]>([])
const skills = ref<SkillItem[]>([])
const loading = ref(true)
const error = ref('')
const busy = ref<string | null>(null)
const notice = ref('')

// ── MCP 安装表单（按目录的 required_parameters 动态生成）──
const installingMcp = ref<McpItem | null>(null)
const installValues = ref<Record<string, string>>({})

function openInstallForm(item: McpItem) {
  installingMcp.value = item
  installValues.value = {}
  notice.value = ''
}

function closeInstallForm() {
  installingMcp.value = null
  installValues.value = {}
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    const res = await authedFetch('/api/catalog')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const data = await res.json()
    mcp.value = data.mcp ?? []
    skills.value = data.skills ?? []
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function installMcp() {
  const item = installingMcp.value
  if (!item) return
  busy.value = item.id
  notice.value = ''
  try {
    const res = await authedFetch('/api/catalog/mcp/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: item.id, values: installValues.value }),
    })
    const data = await res.json()
    if (!res.ok) throw new Error(data.message ?? `HTTP ${res.status}`)
    notice.value = `已安装 MCP「${item.name}」，重启引擎或新开会话后生效`
    closeInstallForm()
    await load()
  } catch (e) {
    notice.value = `安装失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    busy.value = null
  }
}

async function installSkill(item: SkillItem) {
  busy.value = item.id
  notice.value = ''
  try {
    const res = await authedFetch('/api/catalog/skills/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: item.id }),
    })
    const data = await res.json()
    if (!res.ok) throw new Error(data.message ?? `HTTP ${res.status}`)
    notice.value = `已安装 Skill「${item.name}」，重启引擎或新开会话后生效`
    await load()
  } catch (e) {
    notice.value = `安装失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    busy.value = null
  }
}

onMounted(load)
</script>

<template>
  <div class="page">
    <PageHead title="SKILL / MCP 管理" @back="router.push('/')" />
    <main class="body">
      <div class="tabs">
        <button class="tab" :class="{ on: tab === 'mcp' }" @click="tab = 'mcp'">MCP</button>
        <button class="tab" :class="{ on: tab === 'skills' }" @click="tab = 'skills'">Skills</button>
      </div>

      <p v-if="notice" class="notice">{{ notice }}</p>
      <p v-if="error" class="notice err">加载失败：{{ error }}</p>
      <p v-if="loading" class="hint">加载中…</p>

      <!-- MCP -->
      <template v-if="tab === 'mcp'">
        <p class="sec-label">已收录的 MCP Server（点击安装进入配置）</p>
        <div class="group">
          <button
            v-for="item in mcp"
            :key="item.id"
            class="row"
            :disabled="busy !== null"
            @click="item.installed ? null : openInstallForm(item)"
          >
            <span class="ri" :class="{ on: item.installed }"><CoomiIcon name="plug" :size="17" /></span>
            <span class="rt">
              <span class="rmain">
                {{ item.name }}
                <span v-if="item.installed" class="badge" :class="item.enabled ? 'ok' : 'off'">
                  {{ item.enabled ? '已启用' : '已停用' }}
                </span>
              </span>
              <span class="rsub">{{ item.description }} · {{ item.transport }}</span>
            </span>
            <span v-if="busy === item.id" class="hint">安装中…</span>
            <CoomiIcon v-else-if="item.installed" name="check" :size="17" class="tick" />
            <CoomiIcon v-else name="plus" :size="17" class="tick" />
          </button>
        </div>
      </template>

      <!-- Skills -->
      <template v-else>
        <p class="sec-label">已收录的 Skill（点击安装）</p>
        <div class="group">
          <button
            v-for="item in skills"
            :key="item.id"
            class="row"
            :disabled="busy !== null || item.installed"
            @click="installSkill(item)"
          >
            <span class="ri" :class="{ on: item.installed }"><CoomiIcon name="wrench" :size="17" /></span>
            <span class="rt">
              <span class="rmain">
                {{ item.name }}
                <span v-if="item.installed" class="badge ok">已安装</span>
              </span>
              <span class="rsub">{{ item.description }}</span>
            </span>
            <span v-if="busy === item.id" class="hint">安装中…</span>
            <CoomiIcon v-else-if="item.installed" name="check" :size="17" class="tick" />
            <CoomiIcon v-else name="plus" :size="17" class="tick" />
          </button>
        </div>
      </template>

      <!-- MCP 安装参数表单 -->
      <div v-if="installingMcp" class="sheet-mask" @click.self="closeInstallForm">
        <div class="sheet">
          <p class="sheet-title">配置 {{ installingMcp.name }}</p>
          <p class="hint">{{ installingMcp.description }}</p>
          <label v-for="p in installingMcp.required_parameters" :key="p.key" class="field">
            <span>{{ p.label }}</span>
            <input
              v-model="installValues[p.key]"
              :type="p.secret ? 'password' : 'text'"
              :placeholder="p.key"
              autocomplete="off"
            />
          </label>
          <div v-if="installingMcp.required_parameters.length === 0" class="hint">该 MCP 无需额外配置，直接安装。</div>
          <div class="sheet-actions">
            <button class="btn ghost" @click="closeInstallForm">取消</button>
            <button class="btn primary" :disabled="busy !== null" @click="installMcp">
              {{ busy === installingMcp.id ? '安装中…' : '安装' }}
            </button>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}
.tab {
  flex: 1;
  min-height: 40px;
  border-radius: var(--r-md);
  background: var(--fill-strong);
  color: var(--text-2);
  font-size: 14px;
  font-weight: 550;
}
.tab.on {
  background: var(--blue-soft);
  color: var(--blue);
}
.notice {
  margin: 0 0 10px;
  padding: 10px 12px;
  border-radius: var(--r-sm);
  background: var(--ok-soft);
  color: var(--ok);
  font-size: 13px;
}
.notice.err {
  background: var(--danger-soft);
  color: var(--danger);
}
.hint { margin: 4px 0 0; color: var(--text-3); font-size: 13px; }
.badge {
  display: inline-block;
  margin-left: 6px;
  padding: 1px 8px;
  border-radius: var(--r-pill);
  font-size: 11px;
  font-weight: 600;
  vertical-align: 2px;
}
.badge.ok { background: var(--ok-soft); color: var(--ok); }
.badge.off { background: var(--fill-press); color: var(--text-3); }
.sheet-mask {
  position: fixed; inset: 0; z-index: 60;
  background: rgba(0, 0, 0, 0.4);
  display: flex; align-items: flex-end;
}
.sheet {
  width: 100%;
  background: var(--bg-card);
  border-radius: 18px 18px 0 0;
  padding: 18px 16px calc(16px + var(--safe-bottom));
}
.sheet-title { font-size: 16px; font-weight: 650; margin-bottom: 4px; }
.field { display: block; margin-top: 12px; }
.field span { display: block; margin-bottom: 6px; color: var(--text-2); font-size: 13px; }
.field input {
  width: 100%;
  min-height: 44px;
  padding: 0 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--r-sm);
  background: var(--bg-input);
  color: var(--text);
  font-size: 15px;
}
.sheet-actions { display: flex; gap: 10px; margin-top: 18px; }
.sheet-actions .btn { flex: 1; }
.btn.primary { background: var(--blue); color: #fff; }
.btn.ghost { background: var(--fill-strong); color: var(--text); }
.btn:disabled { opacity: 0.6; }
</style>
