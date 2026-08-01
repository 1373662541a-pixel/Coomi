<script setup lang="ts">
/**
 * 消息气泡。
 *
 * 助手消息按段落切块渲染 —— 这是「瀑布流」的关键：
 * 已经写完的段落是稳定 DOM，只有最后一块随 token 重绘，
 * 新段落出现时自己做一次 8px 上浮。整条消息整体重排会闪，切块之后不会。
 * marked 的调用同时被 60ms 节流，流式期间不会一秒解析几十次 markdown。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import type { AssistantMessage, UserMessage } from '@/stores/viewModel'
import { renderMarkdown } from '@/utils/markdown'
import CoomiIcon from './CoomiIcon.vue'

const props = defineProps<{ msg: AssistantMessage | UserMessage }>()

const RATE = 60
const blocks = ref<string[]>([])
const copied = ref(false)
let timer: ReturnType<typeof setTimeout> | null = null
let last = 0

const isUser = computed(() => props.msg.kind === 'user')
const streaming = computed(() => props.msg.kind === 'assistant' && props.msg.streaming)
const src = computed(() => (props.msg.kind === 'assistant' ? props.msg.content : ''))

/** 按空行切块，但围栏代码块整体保留。 */
function splitBlocks(text: string): string[] {
  const out: string[] = []
  let buf: string[] = []
  let fence: string | null = null
  const flush = () => {
    const t = buf.join('\n').trim()
    if (t) out.push(t)
    buf = []
  }
  for (const line of text.split('\n')) {
    const m = /^\s*(```+|~~~+)/.exec(line)
    if (fence) {
      buf.push(line)
      if (m && line.trim().startsWith(fence)) { fence = null; flush() }
      continue
    }
    if (m) { flush(); fence = m[1]; buf.push(line); continue }
    if (line.trim() === '') { flush(); continue }
    buf.push(line)
  }
  flush()
  return out
}

function rebuild() {
  blocks.value = splitBlocks(src.value).map(renderMarkdown)
}

function schedule() {
  if (props.msg.kind !== 'assistant') return
  if (!props.msg.streaming) {
    if (timer) { clearTimeout(timer); timer = null }
    last = Date.now()
    rebuild()
    return
  }
  if (timer) return
  const wait = Math.max(0, RATE - (Date.now() - last))
  timer = setTimeout(() => { timer = null; last = Date.now(); rebuild() }, wait)
}

watch(src, schedule, { immediate: true })
watch(streaming, schedule)
onBeforeUnmount(() => { if (timer) clearTimeout(timer) })

async function copyAll() {
  try { await navigator.clipboard.writeText(src.value) } catch { /* 剪贴板不可用就算了 */ }
  copied.value = true
  setTimeout(() => { copied.value = false }, 1400)
}
</script>

<template>
  <div v-if="isUser" class="row user">
    <div class="bubble cascade">{{ msg.content }}</div>
  </div>

  <div v-else class="assistant">
    <div v-for="(h, i) in blocks" :key="i" class="md blk cascade" v-html="h" />
    <span v-if="streaming" class="stream-caret" />
    <div v-if="!streaming && blocks.length" class="acts">
      <button class="act" @click="copyAll">
        <CoomiIcon :name="copied ? 'check' : 'copy'" :size="15" />
        <span>{{ copied ? '已复制' : '复制' }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.row { display: flex; }
.row.user { justify-content: flex-end; }
.bubble {
  max-width: 84%; padding: 10px 15px;
  border-radius: 19px 19px 7px 19px;
  background: var(--blue); color: #fff;
  font-size: 15.5px; line-height: 1.55; word-break: break-word;
  white-space: pre-wrap;
}

.assistant { max-width: 100%; color: var(--text); }
.blk + .blk { margin-top: 10px; }

.acts { display: flex; gap: 4px; margin-top: 8px; }
.act {
  display: inline-flex; align-items: center; gap: 5px;
  height: 30px; padding: 0 10px;
  border: 0; border-radius: var(--r-pill); background: none;
  font-size: 12.5px; color: var(--text-3);
}
.act:active { background: var(--fill); color: var(--blue); }
</style>

