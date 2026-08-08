<script setup lang="ts">
import { computed, ref } from 'vue'
import type { NoticeItem } from '@/stores/viewModel'
import { useConfigStore } from '@/stores/config'
import CoomiIcon from './CoomiIcon.vue'

const props = defineProps<{ notice: NoticeItem }>()

const config = useConfigStore()
const open = ref(false)
const confirm = ref(false)
const sending = ref(false)
const sent = ref<'ok' | 'fail' | null>(null)

const icon = computed(() => {
  switch (props.notice.tone) {
    case 'error': return 'alert'
    case 'warn': return 'alert'
    case 'success': return 'check'
    default: return ''
  }
})

function toggle() { if (props.notice.detail) open.value = !open.value }

/**
 * 上报报错：仅上传报错日志 + 设备诊断，不含任何对话内容。
 * 双通道：自建端点（国内可达）优先，随后尝试 GitHub issue（失败静默）。
 */
async function sendFeedback() {
  if (sending.value) return
  sending.value = true
  sent.value = null
  const now = new Date().toISOString()
  let diagnostics = '{}'
  try {
    const raw = window.CoomiAndroid?.getDiagnostics?.()
    if (raw) diagnostics = raw
  } catch { /* 原生桥不可用时忽略 */ }
  const payload = {
    error: props.notice.text,
    detail: props.notice.detail ?? '',
    diagnostics,
    provider: config.currentProviderId,
    model: config.currentModel,
    permission_mode: config.permissionMode,
    time: now,
  }
  let ok = false
  try {
    const res = await fetch('https://updates.septemc.com/coomi/feedback/api.php', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
    ok = res.ok
  } catch { ok = false }
  // GitHub issue 通道：用户在设置页填过 token 才尝试；失败静默忽略。
  try {
    const token = localStorage.getItem('coomi.githubToken')
    if (token) {
      await fetch('https://api.github.com/repos/TensorHub-ORG/Coomi-Android/issues', {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Accept': 'application/vnd.github+json',
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          title: `[报错反馈] ${(props.notice.text || 'Coomi 错误').slice(0, 60)}`,
          body: `> 仅包含报错日志与设备诊断，不含对话内容。\n\n**错误**\n${props.notice.text}\n\n**详情**\n${props.notice.detail || '(无)'}\n\n**诊断**\n\`\`\`json\n${diagnostics}\n\`\`\`\n\n**时间** ${now}`,
        }),
      })
    }
  } catch { /* 忽略 */ }
  sending.value = false
  sent.value = ok ? 'ok' : 'fail'
  // 已发送提示停留片刻后自动收起
  if (ok) setTimeout(() => { sent.value = null; confirm.value = false }, 2600)
}
</script>

<template>
  <div class="notice cascade" :class="notice.tone" @click="toggle">
    <CoomiIcon v-if="icon" :name="icon" :size="14" />
    <span>{{ notice.text }}</span>
    <CoomiIcon v-if="notice.detail" name="chevronRight" :size="14" class="chev" :class="{ open }" />
  </div>
  <div v-if="notice.detail && open" class="notice-detail cascade">
    <pre>{{ notice.detail }}</pre>
  </div>

  <div v-if="notice.tone === 'error'" class="fb">
    <template v-if="!confirm">
      <button class="fb-btn" @click.stop="confirm = true">
        <CoomiIcon name="send" :size="13" />
        <span>反馈报错</span>
      </button>
    </template>
    <template v-else-if="sent === null">
      <div class="fb-confirm">
        <span>本次反馈仅上传报错日志与设备信息，<b>不含任何对话内容</b>。确认上报？</span>
        <div class="fb-actions">
          <button class="fb-btn ghost" @click.stop="confirm = false; sent = null">取消</button>
          <button class="fb-btn" :disabled="sending" @click.stop="sendFeedback()">
            {{ sending ? '上传中…' : '确认上传' }}
          </button>
        </div>
      </div>
    </template>
    <template v-else>
      <span class="fb-result" :class="sent">{{ sent === 'ok' ? '已收到，感谢反馈' : '上传失败，可稍后重试' }}</span>
    </template>
  </div>
</template>

<style scoped>
.notice {
  align-self: center; display: inline-flex; align-items: center; gap: 6px;
  min-width: 0; max-width: 92%; padding: 6px 14px;
  border-radius: var(--r-pill); background: var(--fill);
  font-size: 12.5px; line-height: 1.5; color: var(--text-3);
}
.notice span { min-width: 0; max-width: 100%; overflow-wrap: anywhere; word-break: break-word; }
.notice.warn { background: var(--orange-soft); color: var(--orange); }
.notice.success { background: var(--ok-soft); color: var(--ok); }
/* 报错：红字、无背景底框、无圆角——只保留文字颜色区分。 */
.notice.error {
  align-self: stretch; width: 100%; max-width: 100%; align-items: flex-start;
  background: transparent; color: var(--danger);
  text-align: left; word-break: break-word;
}
.notice.error :deep(svg) { flex-shrink: 0; margin-top: 1px; color: var(--danger); }
.chev { flex-shrink: 0; transition: transform .18s; }
.chev.open { transform: rotate(90deg); }

.notice-detail {
  align-self: center; width: 100%; max-width: 92%;
  margin-top: -4px; padding: 9px 13px;
  border-radius: var(--r-md); background: var(--code-bg);
}
.notice-detail pre {
  margin: 0; font-family: var(--font-mono); font-size: 11.8px; line-height: 1.6;
  color: var(--code-text); white-space: pre-wrap; word-break: break-word;
  max-height: 260px; overflow-y: auto;
}

/* 报错反馈按钮（仅 error 通知显示） */
.fb {
  align-self: center; width: 100%; max-width: 92%;
  display: flex; flex-direction: column; gap: 8px;
  margin-top: -2px;
}
.fb-btn {
  display: inline-flex; align-items: center; gap: 5px;
  align-self: flex-start; height: 30px; padding: 0 12px;
  border: 1px solid var(--border); border-radius: var(--r-pill);
  background: var(--bg); color: var(--text-2);
  font-size: 12.5px; font-weight: 600;
}
.fb-btn:active { background: var(--fill); }
.fb-btn:disabled { opacity: .6; }
.fb-btn.ghost { color: var(--text-3); }
.fb-confirm {
  padding: 10px 12px; border: 1px solid var(--border);
  border-radius: var(--r-md); background: var(--bg);
  font-size: 12.5px; line-height: 1.55; color: var(--text-2); text-align: left;
}
.fb-confirm b { color: var(--danger); }
.fb-actions { display: flex; gap: 8px; margin-top: 8px; }
.fb-result { font-size: 12.5px; font-weight: 600; }
.fb-result.ok { color: var(--ok); }
.fb-result.fail { color: var(--danger); }
</style>
