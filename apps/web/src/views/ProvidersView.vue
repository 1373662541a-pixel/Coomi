<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import PageHead from '@/components/PageHead.vue'
import CoomiIcon from '@/components/CoomiIcon.vue'
import { useConfigStore, type ProviderConfig, type ProviderStatus } from '@/stores/config'

const router = useRouter()
const config = useConfigStore()

const providers = computed(() => config.mergedProviders)

onMounted(() => { void config.fetchProviders() })

function statusLabel(status?: ProviderStatus): string {
  if (status === 'current') return '当前'
  if (status === 'configured') return '已配置'
  return '未配置'
}

function statusClass(status?: ProviderStatus): string {
  return status === 'current' ? 'current' : status === 'configured' ? 'configured' : 'unconfigured'
}

function openProvider(provider: ProviderConfig) {
  router.push(`/providers/${encodeURIComponent(provider.id)}`)
}

function backToDashboard() {
  if (window.CoomiAndroid?.openDashboard) window.CoomiAndroid.openDashboard()
  else router.push('/')
}
</script>

<template>
  <div class="page">
    <PageHead title="供应商" @back="backToDashboard">
      <template #right>
        <button class="icon-btn blue" aria-label="添加供应商" @click="router.push('/providers/new')">
          <CoomiIcon name="plus" />
        </button>
      </template>
    </PageHead>

    <main class="body">
      <p v-if="config.usingMock" class="banner">
        <CoomiIcon name="alert" :size="15" />
        <span>后端未连接，下面是本地示例数据，修改不会保存。</span>
      </p>
      <p v-if="config.loading" class="hint">加载中...</p>

      <div class="group">
        <button
          v-for="provider in providers"
          :key="provider.id"
          class="provider-row"
          @click="openProvider(provider)"
        >
          <span class="tile" :class="{ on: provider.status === 'current', ready: provider.status === 'configured' }">
            <CoomiIcon name="key" :size="18" />
          </span>
          <span class="row-text">
            <span class="name">{{ provider.name }}</span>
            <span class="meta">{{ provider.id }}<template v-if="provider.models.length"> · {{ provider.models.length }} 个模型</template></span>
          </span>
          <span class="status" :class="statusClass(provider.status)">{{ statusLabel(provider.status) }}</span>
          <CoomiIcon name="chevronRight" :size="16" class="arrow" />
        </button>
      </div>

      <p v-if="!config.loading && providers.length === 0" class="hint">还没有供应商配置。</p>
      <p class="note">内置供应商会一直显示；只有保存配置后才会写入应用配置。</p>
    </main>
  </div>
</template>

<style scoped>
.page { display: flex; flex-direction: column; height: 100%; background: var(--page); }
.body { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 12px calc(var(--safe-bottom) + 24px); }
.icon-btn.blue { color: var(--blue); }
.banner {
  display: flex; align-items: flex-start; gap: 7px; margin-bottom: 12px;
  padding: 10px 12px; border-radius: var(--r-md); background: var(--orange-soft);
  color: #8a4a30; font-size: 12.8px; line-height: 1.55;
}
.banner :deep(svg) { flex-shrink: 0; margin-top: 1px; color: var(--orange); }
.hint { padding: 4px; text-align: center; font-size: 13px; line-height: 1.65; color: var(--text-3); }
.group { overflow: hidden; border-radius: var(--r-card); background: var(--bg); box-shadow: var(--shadow-1); }
.provider-row {
  display: flex; align-items: center; gap: 11px; width: 100%; min-height: 66px;
  padding: 11px 13px; text-align: left; background: var(--bg);
}
.provider-row + .provider-row { border-top: 1px solid var(--border); }
.provider-row:active { background: var(--fill); }
.tile {
  display: grid; place-items: center; flex-shrink: 0; width: 36px; height: 36px;
  border-radius: 10px; background: var(--fill-strong); color: var(--text-2);
}
.tile.ready { color: var(--ok); background: var(--ok-soft); }
.tile.on { color: var(--blue); background: var(--blue-soft); }
.row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14.5px; font-weight: 600; color: var(--text); }
.meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); font-size: 11.5px; color: var(--text-3); }
.status { flex-shrink: 0; padding: 3px 8px; border-radius: var(--r-pill); font-size: 11px; font-weight: 650; }
.status.unconfigured { color: var(--text-3); background: var(--fill); }
.status.configured { color: var(--ok); background: var(--ok-soft); }
.status.current { color: var(--blue); background: var(--blue-soft); }
.arrow { flex-shrink: 0; color: var(--text-3); }
.note { margin: 14px 4px 0; font-size: 12px; line-height: 1.7; color: var(--text-3); }
</style>
