<script setup lang="ts">
/**
 * 工具调用卡片。
 *
 * 折叠态一行说清「谁 / 对什么 / 结果如何」，展开态才给参数、内容、diff、输出。
 * 状态是有语义色的：运行中蓝 + 左侧流光，成功绿勾，失败红叉且输出染红，
 * 待授权橙（真正的确认交给底部 ApprovalSheet，卡片只说明原因），缓存命中灰闪电。
 */
import { computed, ref } from 'vue'
import type { ToolCard } from '@/stores/viewModel'
import { asText, toolMeta, toolTarget } from '@/utils/toolMeta'
import CoomiIcon from './CoomiIcon.vue'

const props = defineProps<{ card: ToolCard }>()

/** 大字段单独成块，不塞进参数表。 */
const BIG = new Set(['content', 'old_string', 'new_string', 'prompt'])

const manual = ref<boolean | null>(null)
const full = ref(false)
const copied = ref(false)

const meta = computed(() => toolMeta(props.card.toolName))
const target = computed(() => toolTarget(props.card.arguments))
const asStr = asText

const st = computed(() => {
  switch (props.card.status) {
    case 'success': return { label: '', cls: 'ok', spin: false, icon: 'check' }
    case 'error': return { label: '失败', cls: 'err', spin: false, icon: 'close' }
    case 'cancelled': return { label: '已取消', cls: 'cancelled', spin: false, icon: 'close' }
    case 'awaiting_approval': return { label: '待授权', cls: 'wait', spin: false, icon: 'shield' }
    case 'cache_hit': return { label: '缓存', cls: 'cache', spin: false, icon: 'bolt' }
    case 'starting': return { label: '准备', cls: 'run', spin: true, icon: '' }
    default: return { label: '运行中', cls: 'run', spin: true, icon: '' }
  }
})

const argRows = computed(() =>
  Object.entries(props.card.arguments ?? {})
    .filter(([k, v]) => !BIG.has(k) && v != null && v !== '')
    .map(([k, v]) => ({ k, v: asStr(v) })),
)

const contentArg = computed(() => asStr(props.card.arguments?.content) || asStr(props.card.arguments?.prompt))
const oldStr = computed(() => asStr(props.card.arguments?.old_string))
const newStr = computed(() => asStr(props.card.arguments?.new_string))
const isDiff = computed(() => Boolean(oldStr.value || newStr.value))
const output = computed(() => props.card.resultPreview ?? '')

const diffLines = computed(() => {
  const out: { sign: '-' | '+'; text: string }[] = []
  if (oldStr.value) for (const t of oldStr.value.split('\n')) out.push({ sign: '-', text: t })
  if (newStr.value) for (const t of newStr.value.split('\n')) out.push({ sign: '+', text: t })
  return out
})

const hasBody = computed(() => argRows.value.length > 0 || Boolean(contentArg.value) || isDiff.value || Boolean(output.value) || Boolean(props.card.riskSummary))
const open = computed(() => manual.value ?? props.card.expanded ?? false)
const long = computed(() => output.value.length > 700 || output.value.split('\n').length > 14)

function toggle() { if (hasBody.value) manual.value = !open.value }

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    // WebView 里 clipboard API 偶尔不可用，退回老办法
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    try { document.execCommand('copy') } catch { /* 放弃 */ }
    document.body.removeChild(ta)
  }
  copied.value = true
  setTimeout(() => { copied.value = false }, 1400)
}
</script>

<template>
  <div class="tool" :class="[st.cls, { open }]">
    <button class="head" :class="{ tapable: hasBody }" @click="toggle">
      <span class="tile" :class="st.cls">
        <CoomiIcon :name="meta.icon" :size="17" />
        <span v-if="st.spin" class="ring" />
      </span>

      <span class="txt">
        <span class="verb">{{ meta.verb }}</span>
        <code v-if="target" class="target">{{ target }}</code>
      </span>

      <span class="st" :class="st.cls">
        <CoomiIcon v-if="st.icon" :name="st.icon" :size="14" />
        <span v-if="st.label">{{ st.label }}</span>
        <span v-if="card.elapsed != null" class="ms">{{ card.elapsed.toFixed(1) }}s</span>
      </span>

      <CoomiIcon v-if="hasBody" name="chevronRight" :size="14" class="chev" :class="{ open }" />
    </button>

    <div v-if="card.status === 'awaiting_approval'" class="risk">
      <CoomiIcon name="alert" :size="15" />
      <span>{{ card.riskSummary || '需要你授权后才会执行' }}<template v-if="card.access"> · {{ card.access }}</template></span>
    </div>

    <div v-if="open && hasBody" class="body">
      <div v-if="argRows.length" class="sec">
        <p class="slabel">参数</p>
        <div class="args">
          <div v-for="r in argRows" :key="r.k" class="arg">
            <span class="ak">{{ r.k }}</span>
            <code class="av">{{ r.v }}</code>
          </div>
        </div>
      </div>

      <div v-if="isDiff" class="sec">
        <p class="slabel">改动</p>
        <div class="diff">
          <div v-for="(l, i) in diffLines" :key="i" class="dl" :class="l.sign === '+' ? 'add' : 'del'">
            <span class="dsign">{{ l.sign }}</span><span class="dtext">{{ l.text || ' ' }}</span>
          </div>
        </div>
      </div>

      <div v-if="contentArg && !isDiff" class="sec">
        <div class="sbar">
          <p class="slabel">内容</p>
          <button class="mini" @click.stop="copy(contentArg)">{{ copied ? '已复制' : '复制' }}</button>
        </div>
        <pre class="mono">{{ contentArg }}</pre>
      </div>

      <div v-if="output" class="sec">
        <div class="sbar">
          <p class="slabel">{{ card.isError ? '错误输出' : '输出' }}</p>
          <button class="mini" @click.stop="copy(output)">{{ copied ? '已复制' : '复制' }}</button>
        </div>
        <pre class="mono out" :class="{ err: card.isError, clip: long && !full }">{{ output }}</pre>
        <button v-if="long" class="more" @click.stop="full = !full">
          {{ full ? '收起' : '展开全部' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tool {
  border: 1px solid var(--border); border-radius: var(--r-md);
  background: var(--bg); overflow: hidden;
  transition: border-color .16s, background .16s;
}
.tool.run { border-color: var(--blue-border); }
.tool.err { border-color: var(--danger-border); }
.tool.wait { border-color: var(--orange-border); background: var(--orange-soft); }

/* 运行中：左边一条呼吸的光带，比转圈更容易在长时间线里被扫到。 */
.tool.run { position: relative; }
.tool.run::before {
  content: ''; position: absolute; inset: 0 auto 0 0; width: 2px;
  background: linear-gradient(180deg, transparent, var(--blue), transparent);
  animation: glow 1.4s ease-in-out infinite;
}
@keyframes glow { 0%, 100% { opacity: .3; } 50% { opacity: 1; } }

.head {
  display: flex; align-items: center; gap: 10px;
  width: 100%; min-height: 46px; padding: 8px 11px;
  border: 0; background: none; text-align: left;
}
.head.tapable:active { background: var(--fill); }

.tile {
  position: relative; display: grid; place-items: center; flex-shrink: 0;
  width: 30px; height: 30px; border-radius: 9px;
  background: var(--fill-strong); color: var(--text-2);
}
.tile.run { background: var(--blue-soft); color: var(--blue); }
.tile.ok { background: var(--ok-soft); color: var(--ok); }
.tile.err { background: var(--danger-soft); color: var(--danger); }
.tile.wait { background: var(--bg); color: var(--orange); }
.tile.cancelled { background: var(--fill-strong); color: var(--text-3); }
.ring {
  position: absolute; inset: -3px;
  border: 1.6px solid var(--blue-border); border-top-color: var(--blue);
  border-radius: 12px;
  animation: coomi-spin .8s linear infinite;
}

.txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.verb { font-size: 13.8px; font-weight: 600; color: var(--text); }
.target {
  font-family: var(--font-mono); font-size: 11.6px; color: var(--text-2);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

.st {
  display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0;
  font-size: 11.5px; font-weight: 600; color: var(--text-3);
}
.st.run { color: var(--blue); }
.st.ok { color: var(--ok); }
.st.err { color: var(--danger); }
.st.wait { color: var(--orange); }
.st.cancelled { color: var(--text-3); }
.ms { font-weight: 400; color: var(--text-3); }
.chev { flex-shrink: 0; color: var(--text-3); transition: transform .18s; }
.chev.open { transform: rotate(90deg); }

.risk {
  display: flex; align-items: flex-start; gap: 7px;
  padding: 8px 12px 10px; border-top: 1px solid var(--orange-border);
  font-size: 12.5px; line-height: 1.55; color: #8a4a30;
}
.risk :deep(svg) { flex-shrink: 0; margin-top: 1px; color: var(--orange); }

.body { border-top: 1px solid var(--border); padding: 4px 11px 11px; }
.sec { margin-top: 9px; }
.sbar { display: flex; align-items: center; justify-content: space-between; }
.slabel { font-size: 10.5px; font-weight: 600; letter-spacing: .06em; color: var(--text-3); text-transform: uppercase; }
.mini {
  padding: 3px 9px; border: 0; border-radius: var(--r-sm);
  background: none; font-size: 11.5px; font-weight: 600; color: var(--blue);
}
.mini:active { background: var(--blue-soft); }

.args { margin-top: 5px; display: flex; flex-direction: column; gap: 4px; }
.arg { display: flex; gap: 8px; font-size: 12px; line-height: 1.5; }
.ak { flex-shrink: 0; min-width: 68px; color: var(--text-3); }
.av {
  flex: 1; min-width: 0; font-family: var(--font-mono); font-size: 11.6px;
  color: var(--code-text); word-break: break-all;
}

.mono {
  margin-top: 6px; padding: 9px 10px;
  border-radius: var(--r-sm); background: var(--code-bg);
  font-family: var(--font-mono); font-size: 11.8px; line-height: 1.6;
  color: var(--code-text); white-space: pre-wrap; word-break: break-word;
  overflow-x: auto;
}
.out.err { background: var(--danger-soft); color: #9b3a2c; }
.out.clip { max-height: 210px; overflow: hidden; mask-image: linear-gradient(180deg, #000 72%, transparent); }
.more {
  width: 100%; margin-top: 6px; padding: 7px 0;
  border: 0; border-radius: var(--r-sm); background: var(--fill);
  font-size: 12px; font-weight: 600; color: var(--text-2);
}
.more:active { background: var(--fill-press); }

.diff {
  margin-top: 6px; border-radius: var(--r-sm);
  background: var(--code-bg); overflow: hidden;
  font-family: var(--font-mono); font-size: 11.6px; line-height: 1.62;
}
.dl { display: flex; gap: 6px; padding: 0 8px; }
.dl.del { background: #fdecea; color: #99392c; }
.dl.add { background: #e9f7ef; color: #1c7a52; }
.dsign { flex-shrink: 0; opacity: .55; }
.dtext { flex: 1; white-space: pre-wrap; word-break: break-word; }
</style>

