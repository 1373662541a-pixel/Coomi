<script setup lang="ts">
import { computed } from 'vue'
import type { NoticeItem } from '@/stores/viewModel'
import CoomiIcon from './CoomiIcon.vue'

const props = defineProps<{ notice: NoticeItem }>()

const icon = computed(() => {
  switch (props.notice.tone) {
    case 'error': return 'alert'
    case 'warn': return 'alert'
    case 'success': return 'check'
    default: return ''
  }
})
</script>

<template>
  <div class="notice cascade" :class="notice.tone">
    <CoomiIcon v-if="icon" :name="icon" :size="14" />
    <span>{{ notice.text }}</span>
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
.notice.error {
  align-self: stretch; width: 100%; max-width: 100%; align-items: flex-start; overflow: hidden;
  padding: 10px 13px; border-radius: var(--r-md);
  background: var(--danger-soft); color: #9b3a2c;
  text-align: left; word-break: break-word;
}
.notice.error :deep(svg) { flex-shrink: 0; margin-top: 1px; color: var(--danger); }
</style>
