import { defineStore } from 'pinia'
import { ref, computed, shallowRef } from 'vue'
import { createTransport, type Transport } from '@/bridge'
import { isDemoMode } from '@/bridge/demoMode'
import type { AgentEvent } from '@/protocol/events'
import type { InboundEnvelope } from '@/protocol/commands'
import { nextId } from '@/bridge/envelope'
import { useConnectionStore } from './connection'
import { useConfigStore } from './config'
import { useSessionsStore } from './sessions'
import type { AssistantMessage, LoopProgress, QuestionCard, ReasoningBlock, RunState, Timelineitem, ToolCard } from './viewModel'

export const useSessionStore = defineStore('session', () => {
  const connection = useConnectionStore()
  const config = useConfigStore()
  const sessions = useSessionsStore()

  const sessionId = ref(createSessionId())
  const timeline = ref<Timelineitem[]>([])
  const runState = ref<RunState>('idle')
  const usage = ref<{
    total: number; input: number; output: number; contextRatio: number
    contextUsed: number; contextWindow: number
  } | null>(null)
  const loop = ref<LoopProgress>({ active: false, currentStep: 0, totalSteps: 0, status: '' })

  let currentAssistant: AssistantMessage | null = null
  let connectedSessionId = ''
  let persistTimer: ReturnType<typeof setTimeout> | null = null
  const transport = shallowRef<Transport | null>(null)

  const isBusy = computed(() => runState.value !== 'idle')
  const pendingApproval = computed(() => timeline.value.find((t): t is ToolCard => t.kind === 'tool' && t.status === 'awaiting_approval'))
  const pendingQuestion = computed(() => timeline.value.find((t): t is QuestionCard => t.kind === 'question' && !t.answered))

  /** 时间线写回 localStorage 有节流：流式期间不要每个 chunk 都序列化。 */
  function persistSoon() {
    if (isDemoMode()) return // 演示内容不该混进真实历史
    if (persistTimer) return
    persistTimer = setTimeout(() => {
      persistTimer = null
      const items = timeline.value.filter(t => t.kind !== 'notice')
      if (items.length === 0) return
      sessions.touch(sessionId.value, { turns: timeline.value.filter(t => t.kind === 'user').length })
      sessions.saveTranscript(sessionId.value, timeline.value)
    }, 1200)
  }

  function flushPersistence() {
    if (persistTimer) {
      clearTimeout(persistTimer)
      persistTimer = null
    }
    if (isDemoMode()) return
    const items = timeline.value.filter(t => t.kind !== 'notice')
    if (items.length === 0) return
    sessions.touch(sessionId.value, { turns: timeline.value.filter(t => t.kind === 'user').length })
    sessions.saveTranscript(sessionId.value, timeline.value)
  }

  /** 换 sessionId 后必须重连：WS 的路径里带着 session id。 */
  function connect(wsUrl?: string) {
    if (transport.value && connectedSessionId === sessionId.value) return
    const targetSessionId = sessionId.value
    const previous = transport.value
    transport.value = null
    connectedSessionId = ''
    previous?.close()
    if (wsUrl) connection.setWsUrl(wsUrl)
    const t = createTransport(targetSessionId, wsUrl)
    transport.value = t
    connectedSessionId = targetSessionId
    t.onStateChange(s => {
      if (transport.value !== t || sessionId.value !== targetSessionId) return
      connection.setState(s)
      if (s === 'open') {
        t.send({ command: 'set_permission_mode', mode: config.permissionMode })
        if (config.currentProviderId && config.currentModel) {
          t.send({ command: 'select_model', provider_id: config.currentProviderId, model: config.currentModel })
        }
      }
    })
    t.onMessage(env => {
      if (transport.value !== t || sessionId.value !== targetSessionId) return
      onInbound(env)
    })
    t.connect()
  }

  function disconnect() { transport.value?.close(); transport.value = null; connectedSessionId = '' }

  function onInbound(env: InboundEnvelope) {
    if (env.type === 'event') applyEvent(env.payload)
    else if (env.type === 'error') pushNotice('error', env.payload.message)
  }

  function applyEvent(ev: AgentEvent) {
    switch (ev.event_type) {
      // 兜底：turn_end 之后又开始吐字（引擎续了一轮），状态得跟着回到忙。
      case 'text_chunk': if (runState.value === 'idle') runState.value = 'thinking'; appendAssistant(ev.content); break
      case 'reasoning_chunk': if (runState.value === 'idle') runState.value = 'thinking'; appendReasoning(ev.content); break
      case 'tool_start':
        endAssistantStream()
        timeline.value.push({ kind: 'tool', callId: ev.call_id, toolName: ev.tool_name, arguments: ev.arguments, status: 'starting', expanded: false })
        runState.value = 'executing'
        break
      case 'tool_running': patchTool(ev.call_id, c => c.status = 'running'); runState.value = 'executing'; break
      case 'tool_done':
        patchTool(ev.call_id, c => { c.status = ev.is_error ? 'error' : 'success'; c.elapsed = ev.elapsed; c.resultPreview = ev.result_preview; c.isError = ev.is_error })
        // 工具跑完不等于一轮结束 —— 模型接着想下一步。回 idle 只认 turn_end /
        // 取消 / 致命错误，否则输入区会在循环中途闪回「下达任务」和发送箭头。
        runState.value = 'thinking'
        break
      case 'tool_cache_hit': patchTool(ev.call_id, c => c.status = 'cache_hit'); break
      case 'tool_approval_request':
        endAssistantStream()
        if (!patchTool(ev.call_id, c => { c.status = 'awaiting_approval'; c.access = ev.access; c.riskSummary = ev.risk_summary; c.expanded = true })) {
          timeline.value.push({ kind: 'tool', callId: ev.call_id, toolName: ev.tool_name, arguments: ev.arguments, status: 'awaiting_approval', access: ev.access, riskSummary: ev.risk_summary, expanded: true })
        }
        runState.value = 'awaiting_approval'
        break
      case 'user_question_request':
        endAssistantStream()
        timeline.value.push({ kind: 'question', callId: ev.call_id, question: ev.question, options: ev.options, allowFreeText: ev.allow_free_text ?? true, answered: false })
        runState.value = 'awaiting_question'
        break
      case 'file_transfer_request':
        if (ev.operation === 'import') {
          window.CoomiAndroid?.importFilesForRequest?.(ev.request_id)
        } else if (ev.path) {
          window.CoomiAndroid?.exportFileForRequest?.(
            ev.request_id,
            ev.path,
            ev.suggested_name ?? ev.path.split('/').pop() ?? 'coomi-export',
          )
        }
        break
      case 'usage_update': {
        const previous = usage.value
        usage.value = {
          total: ev.usage.total_tokens ?? previous?.total ?? 0,
          input: ev.usage.input_tokens ?? previous?.input ?? 0,
          output: ev.usage.output_tokens ?? previous?.output ?? 0,
          contextRatio: ev.usage.context_ratio ?? previous?.contextRatio ?? 0,
          contextUsed: ev.usage.context_used_tokens ?? previous?.contextUsed ?? 0,
          contextWindow: ev.usage.context_window_tokens ?? previous?.contextWindow ?? 0,
        }
        break
      }
      case 'compression': pushNotice('info', `上下文已压缩 ${fmtTokens(ev.before)} → ${fmtTokens(ev.after)}`); break
      case 'connection_retry': connection.setRetry(`${ev.message}（${ev.attempt}/${ev.max_attempts}）`); break
      case 'agent_error': endAssistantStream(); pushNotice(ev.is_fatal ? 'error' : 'warn', ev.message); if (ev.is_fatal) runState.value = 'idle'; break
      case 'agent_cancelled': endAssistantStream(); cancelRunningTools(); pushNotice('warn', '已停止本轮执行'); break
      case 'bg_task_detached': pushNotice('info', `↪ 已转入后台任务 #${ev.task_id}（${ev.tool_name}）`); break
      case 'bg_task_completed': pushNotice(ev.is_error ? 'error' : 'success', `${ev.is_error ? '✕' : '✓'} 后台任务 #${ev.task_id} ${ev.is_error ? '失败' : '完成'}`); break
      case 'loop_progress':
        loop.value = { active: ev.status !== 'done', currentStep: ev.current_step, totalSteps: ev.total_steps, status: ev.status, currentDescription: loop.value.currentDescription }
        break
      case 'loop_step_start':
        loop.value = { ...loop.value, active: true, totalSteps: ev.total_steps, currentStep: ev.step_index, currentDescription: ev.step_description }
        break
      case 'turn_end': endAssistantStream(); cancelRunningTools(); connection.setRetry(null); runState.value = 'idle'; persistSoon(); break
    }
  }

  function cancelRunningTools() {
    // 停止后引擎可能不会逐个补发 tool_done：把仍在运行/准备中的工具卡片
    // 收尾为「已取消」，否则卡片会永远停在旋转的「运行中」状态。
    let changed = false
    for (const item of timeline.value) {
      if (item.kind === 'tool' && (item.status === 'running' || item.status === 'starting')) {
        item.status = 'cancelled'
        item.isError = true
        changed = true
      }
    }
    if (changed) persistSoon()
  }

  function sendMessage(text: string) {
    const trimmed = text.trim()
    if (!trimmed) return
    // 首条用户消息作为会话标题，抽屉里就不会全是「新对话」。
    const isFirst = !timeline.value.some(t => t.kind === 'user')
    if (isFirst) sessions.touch(sessionId.value, { title: sessions.deriveTitle(trimmed) })
    if (isBusy.value) {
      timeline.value.push({ kind: 'user', id: nextId(), content: trimmed })
      transport.value?.send({ command: 'jump_in', text: trimmed })
      persistSoon()
      return
    }
    timeline.value.push({ kind: 'user', id: nextId(), content: trimmed })
    runState.value = 'thinking'
    transport.value?.send({ command: 'send_message', text: trimmed })
    persistSoon()
  }

  function cancel() { transport.value?.send({ command: 'cancel' }) }
  function approve(callId: string, decision: 'allow' | 'deny' | 'always') {
    patchTool(callId, c => { c.status = decision === 'deny' ? 'error' : 'running'; if (decision === 'deny') { c.resultPreview = '（用户拒绝执行）'; c.isError = true } })
    transport.value?.send({ command: 'approve_tool', call_id: callId, decision })
    if (runState.value === 'awaiting_approval') runState.value = 'executing'
  }
  function answerQuestion(callId: string, answer: string) {
    patchQuestion(callId, q => { q.answered = true; q.answer = answer })
    transport.value?.send({ command: 'answer_question', call_id: callId, answer })
    if (runState.value === 'awaiting_question') runState.value = 'thinking'
  }
  function setPermissionMode(mode: 'ask' | 'auto' | 'full') { config.setPermissionMode(mode); transport.value?.send({ command: 'set_permission_mode', mode }) }
  function togglePlanMode() { const entering = !config.planMode; config.togglePlanMode(); transport.value?.send({ command: entering ? 'enter_plan_mode' : 'exit_plan_mode' }) }
  function selectModel(providerId: string, model: string) { config.selectModel(providerId, model); transport.value?.send({ command: 'select_model', provider_id: providerId, model }) }
  function completeFileTransfer(requestId: string, paths: string[]) {
    transport.value?.send({ command: 'file_transfer_result', request_id: requestId, paths })
  }

  function newSession() {
    flushPersistence()
    endAssistantStream(); timeline.value = []; usage.value = null
    loop.value = { active: false, currentStep: 0, totalSteps: 0, status: '' }; runState.value = 'idle'
    sessionId.value = createSessionId()
    connect()
  }

  /**
   * 打开一条历史会话：先把本机记录铺回来，再用同一个 sessionId 重连。
   * 引擎进程还活着就是真的续上了；重启过则只有这份本机记录，
   * 所以补一条提示，避免用户以为模型还记得。
   */
  function openSession(id: string) {
    if (id === sessionId.value) return
    flushPersistence()
    endAssistantStream()
    usage.value = null
    loop.value = { active: false, currentStep: 0, totalSteps: 0, status: '' }
    runState.value = 'idle'
    const targetId = isUuid(id) ? id : sessions.migrateId(id, createSessionId())
    sessionId.value = targetId
    const restored = sessions.loadTranscript(targetId)
    timeline.value = restored
    if (restored.length > 0) {
      timeline.value.push({
        kind: 'notice', id: nextId(), tone: 'info',
        text: '已恢复本机记录。若引擎重启过，模型这边的上下文可能已经清空。',
      })
    }
    connect()
  }

  function deleteSession(id: string) {
    sessions.remove(id)
    if (id === sessionId.value) newSession()
  }

  function appendAssistant(content: string) {
    if (!currentAssistant) {
      timeline.value.push({ kind: 'assistant', id: nextId(), content: '', streaming: true })
      // 必须拿 push 之后数组里的那个对象：ref 会把它包成代理，
      // 直接改 push 进去的原始对象不触发渲染，流式文本就只会停在第一片。
      currentAssistant = timeline.value[timeline.value.length - 1] as AssistantMessage
    }
    currentAssistant.content += content
  }
  function endAssistantStream() { if (currentAssistant) { currentAssistant.streaming = false; currentAssistant = null } }
  function appendReasoning(content: string) {
    const last = timeline.value[timeline.value.length - 1]
    if (last && last.kind === 'reasoning') { (last as ReasoningBlock).content += content }
    else { timeline.value.push({ kind: 'reasoning', id: nextId(), content, expanded: false }) }
  }
  function patchTool(callId: string, fn: (c: ToolCard) => void): boolean {
    for (let i = timeline.value.length - 1; i >= 0; i--) { const t = timeline.value[i]; if (t.kind === 'tool' && t.callId === callId) { fn(t); return true } }
    return false
  }
  function patchQuestion(callId: string, fn: (q: QuestionCard) => void) {
    for (let i = timeline.value.length - 1; i >= 0; i--) { const t = timeline.value[i]; if (t.kind === 'question' && t.callId === callId) { fn(t); return } }
  }
  function pushNotice(tone: 'info' | 'warn' | 'error' | 'success', text: string) { timeline.value.push({ kind: 'notice', id: nextId(), tone, text }) }

  return { sessionId, timeline, runState, usage, loop, isBusy, pendingApproval, pendingQuestion, connect, disconnect, sendMessage, cancel, approve, answerQuestion, setPermissionMode, togglePlanMode, selectModel, completeFileTransfer, newSession, openSession, deleteSession }
})

function fmtTokens(n: number): string { return n >= 1000 ? (n / 1000).toFixed(1) + 'k' : String(n) }

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i

function isUuid(value: string): boolean {
  return UUID_PATTERN.test(value)
}

function createSessionId(): string {
  const cryptoApi = globalThis.crypto
  if (typeof cryptoApi?.randomUUID === 'function') return cryptoApi.randomUUID()
  const bytes = new Uint8Array(16)
  cryptoApi.getRandomValues(bytes)
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}
