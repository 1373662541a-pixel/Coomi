<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import PageHead from '@/components/PageHead.vue'
import CoomiIcon from '@/components/CoomiIcon.vue'
import { useSessionStore } from '@/stores/session'
import { useSessionsStore, type TaskInfo } from '@/stores/sessions'

const router = useRouter()
const session = useSessionStore()
const sessions = useSessionsStore()
let poll: ReturnType<typeof setInterval> | null = null

const active = computed(() => sessions.tasks.filter(task => task.running))
const recent = computed(() => sessions.tasks.filter(task => !task.running).slice(0, 20))

const statusLabels: Record<TaskInfo['status'], string> = {
  queued: '等待执行',
  running: '执行中',
  awaiting_approval: '等待授权',
  awaiting_input: '等待输入',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
  interrupted: '已中断',
}

function openTask(task: TaskInfo) {
  session.openSession(task.session_id)
  router.push('/')
}

function elapsed(startedAt: number): string {
  const seconds = Math.max(0, Math.floor(Date.now() / 1000 - startedAt))
  if (seconds < 60) return `${seconds} 秒`
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟`
  return `${Math.floor(seconds / 3600)} 小时`
}

onMounted(() => {
  void sessions.refreshTasks()
  poll = setInterval(() => sessions.refreshTasks(), 1500)
})
onBeforeUnmount(() => { if (poll) clearInterval(poll) })
</script>

<template>
  <div class="page">
    <PageHead title="任务中心" @back="router.push('/')" />
    <main class="body">
      <div class="summary">
        <span><strong>{{ active.length }}</strong> 个任务运行中</span>
        <span>并发上限 {{ sessions.taskConcurrencyLimit }}</span>
      </div>

      <p class="sec-label">当前任务</p>
      <div v-if="active.length" class="task-list">
        <div v-for="task in active" :key="task.task_id" class="task-row">
          <button class="task-main" @click="openTask(task)">
            <span class="task-title">{{ task.session_title }}</span>
            <span class="task-meta">
              <span class="live-dot" />{{ statusLabels[task.status] }} · {{ elapsed(task.started_at) }}
              <template v-if="task.current_tool"> · {{ task.current_tool }}</template>
            </span>
          </button>
          <button class="stop" aria-label="取消任务" @click="sessions.cancelTask(task.session_id)">
            <CoomiIcon name="stop" :size="17" />
          </button>
        </div>
      </div>
      <p v-else class="empty">当前没有运行中的任务。</p>

      <template v-if="recent.length">
        <p class="sec-label">最近任务</p>
        <div class="task-list">
          <button v-for="task in recent" :key="task.task_id" class="task-row recent" @click="openTask(task)">
            <span class="task-main">
              <span class="task-title">{{ task.session_title }}</span>
              <span class="task-meta" :class="task.status">{{ statusLabels[task.status] }} · {{ elapsed(task.started_at) }}前开始</span>
            </span>
            <CoomiIcon name="chevronRight" :size="17" class="chevron" />
          </button>
        </div>
      </template>
    </main>
  </div>
</template>

<style scoped>
.page { display: flex; flex-direction: column; height: 100%; background: var(--page); }
.body { flex: 1; overflow-y: auto; padding: 10px 12px calc(var(--safe-bottom) + 24px); }
.summary { display: flex; justify-content: space-between; align-items: baseline; min-height: 40px; padding: 8px 4px; color: var(--text-2); font-size: 13px; }
.summary strong { color: var(--blue); font-size: 20px; }
.sec-label { margin: 14px 4px 7px; }
.task-list { background: var(--bg); border: 1px solid var(--border); border-radius: var(--r-card); overflow: hidden; }
.task-row { display: flex; align-items: center; width: 100%; min-height: 66px; text-align: left; }
.task-row + .task-row { border-top: 1px solid var(--border); }
.task-main { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 5px; padding: 11px 6px 11px 14px; text-align: left; }
.task-title { overflow: hidden; color: var(--text); font-size: 14.5px; font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
.task-meta { display: flex; align-items: center; min-width: 0; overflow: hidden; color: var(--text-3); font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
.live-dot { width: 7px; height: 7px; margin-right: 6px; border-radius: 50%; background: var(--blue); animation: pulse 1.4s ease-in-out infinite; }
@keyframes pulse { 50% { opacity: .35; } }
.stop { display: grid; place-items: center; flex: 0 0 44px; width: 44px; height: 44px; margin-right: 6px; border-radius: 50%; color: var(--danger); }
.stop:active { background: var(--danger-soft); }
.recent { padding: 0; }
.recent:active { background: var(--fill); }
.recent .task-main { pointer-events: none; }
.task-meta.failed { color: var(--danger); }
.task-meta.completed { color: var(--ok); }
.chevron { margin-right: 12px; color: var(--text-3); }
.empty { padding: 24px 8px; text-align: center; color: var(--text-3); font-size: 13px; }
</style>
