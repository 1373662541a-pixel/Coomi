<script setup lang="ts">
import { nextTick, ref } from 'vue'
import { useRouter } from 'vue-router'
import PageHead from '@/components/PageHead.vue'
import CoomiIcon from '@/components/CoomiIcon.vue'
import { apiSend } from '@/bridge/http'
import { useSessionStore } from '@/stores/session'

const router = useRouter()
const session = useSessionStore()
const busy = ref(false)
const error = ref('')

const PROMPT = `我准备在 Coomi 的手机虚拟 Linux 环境中进行 Coomi 自定义迭代。

请先检查本地开发环境：
1. 确认当前命令运行在 Runtime V2 的 Debian ProotLinux 中，源码目录为 ~/custom_coomi，持久 Build Kit 挂载为 /opt/coomi-dev。
2. 先运行 coomidev-build doctor；如果缺少硬依赖，请逐项说明缺少的工具、架构或校验问题，不要把目录已创建当作工具链已就绪。
3. 只使用 /opt/coomi-dev/current 中经过版本固定和 SHA-256 校验的 ARM64/glibc 工具链。禁止把 Termux 的 Android-PIE/Bionic 可执行文件混入 Debian 环境，也不要依赖 apt/dpkg 安装关键构建工具。
4. 使用 file、readelf 和加载器信息验证原生工具，尤其不要把官方 Android SDK/NDK 中常见的 x86_64 aapt2、clang、lld 误当作 ARM64 主机工具。

然后带我完成 GitHub 环境配置：
1. 引导我注册或登录 GitHub 账号。
2. 检查并安装 gh CLI。
3. 使用 gh auth login 的设备码流程完成认证。
4. 教我生成 SSH Key，并将公钥添加到 GitHub。
5. 配置 Git 使用 SSH 访问 GitHub，并验证 SSH 连接。
6. 如确有需要，再指导我创建 GitHub Personal Access Token (classic)，说明最小权限、保存方式和安全注意事项；不要在对话、日志或提交内容中暴露 Token。

配置完成后，请按以下顺序验证：
1. 验证 GitHub 登录状态和 SSH 连接。
2. 为 https://github.com/TensorHub-ORG/Coomi 点星。
3. Fork 该仓库的 main 分支。
4. 将仓库的 main 分支克隆到 Coomi 虚拟 Linux 环境中的 ~/custom_coomi。
5. 检查仓库、分支、远程地址和工作区状态。

之后，请按照 coomi-custom-iteration Skill 的规则协助我进行自定义开发。每次修改前先检查项目规则、当前分支和工作区状态；完成后运行适当的测试，并根据我的选择提交 PR 或构建独立的 CoomiDev APK。

若我要在手机本地构建，请先将涉及的构建配置 Linux 化：使用 POSIX 路径与命令，移除 npm.cmd、.cmd、Windows 盘符、反斜杠和 windows-x86_64 硬编码，同时保留 Windows 主机检测与环境变量覆盖。依次执行 doctor、Android APK 冒烟测试、Rust/NDK 冒烟测试、完整构建；完整包必须为 CoomiDev、包名 com.coomidev.android、默认端口 18765，并使用 assets/coomi-agent-dev.png。全部通过后再验证包名和签名并导出 APK。如果真实 ARM64 工具链无法验证，请明确停止本地构建并改用 GitHub Actions，不要绕过检查。`

async function start() {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    await apiSend('/api/custom-iteration/bootstrap', 'POST', {})
    session.newSession()
    localStorage.setItem(`coomi.draft.${session.sessionId}`, PROMPT)
    await router.push('/')
    await nextTick()
    window.dispatchEvent(new CustomEvent('coomi:prefill-draft', {
      detail: { sessionId: session.sessionId, text: PROMPT },
    }))
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

function goDashboard() {
  if (window.CoomiAndroid?.openDashboard) window.CoomiAndroid.openDashboard()
  else router.push('/')
}
</script>

<template>
  <div class="page">
    <PageHead title="自定义迭代（实验）" @back="goDashboard" />
    <main class="body">
      <section class="hero">
        <span class="eyebrow">CoomiDev Workspace</span>
        <h1>全面自定义自己的 CoomiDev</h1>
        <p>在手机虚拟 Linux 中完成源码修改、测试、PR 和独立 APK 构建，使用可诊断的版本化 ARM64 Build Kit，并始终与主应用隔离。</p>
      </section>

      <section class="prep">
        <div class="section-head"><h2>开始前自动准备</h2><span>一次配置，后续直接使用</span></div>
        <div class="feature-list">
          <article class="feature">
            <span class="feature-icon"><CoomiIcon name="shield" :size="17" /></span>
            <span><b>专用 Skill</b><small>内置 ARM64 环境诊断、Linux 化、测试、PR 与 APK 构建规则</small></span>
          </article>
          <article class="feature">
            <span class="feature-icon"><CoomiIcon name="folder" :size="17" /></span>
            <span><b>隔离工作区</b><small><code>~/custom_coomi</code> 与 <code>/opt/coomi-dev</code> 分离持久化</small></span>
          </article>
          <article class="feature">
            <span class="feature-icon"><CoomiIcon name="git" :size="17" /></span>
            <span><b>确认后提交</b><small>提交、推送和创建 PR 前先展示变更摘要</small></span>
          </article>
        </div>
      </section>

      <section class="auth">
        <h2>GitHub 认证</h2>
        <p class="note">首次进入会话后，Agent 会教你使用 <code>gh auth login</code> 设备码流程配置账号，并指导生成 SSH Key。私钥和认证信息只保存在虚拟环境中，不会写入聊天草稿或提交内容。</p>
      </section>
      <p v-if="error" class="error">{{ error }}</p>
      <button class="primary" :disabled="busy" @click="start">
        <CoomiIcon v-if="busy" name="refresh" class="spin" :size="17" />
        <CoomiIcon v-else name="arrowRight" :size="17" />
        {{ busy ? '正在准备…' : '去自定义迭代' }}
      </button>
    </main>
  </div>
</template>

<style scoped>
.page { display:flex; flex-direction:column; height:100%; background:var(--page); }
.body { flex:1; overflow-y:auto; padding:16px 14px calc(var(--safe-bottom) + 28px); }
.hero { position:relative; padding:18px 17px 19px; overflow:hidden; border:1px solid var(--border); border-radius:8px; background:var(--bg); box-shadow:var(--shadow-1); }
.eyebrow { display:block; margin-bottom:7px; color:var(--blue); font-size:10.5px; font-weight:700; letter-spacing:0; }
h1 { margin:0; color:var(--text); font-size:19px; line-height:1.35; }
.hero p { max-width:34em; margin:7px 0 0; color:var(--text-2); font-size:12.8px; line-height:1.65; }
.prep { margin-top:19px; }
.section-head { display:flex; align-items:baseline; justify-content:space-between; gap:10px; margin:0 3px 8px; }
.section-head h2, .auth h2 { margin:0; color:var(--text); font-size:13px; font-weight:650; }
.section-head > span { color:var(--text-3); font-size:10.5px; }
.feature-list { overflow:hidden; border:1px solid var(--border); border-radius:8px; background:var(--bg); }
.feature { display:grid; grid-template-columns:34px minmax(0,1fr); align-items:center; gap:11px; min-height:70px; padding:11px 13px; }
.feature + .feature { border-top:1px solid var(--border); }
.feature-icon { display:grid; place-items:center; width:32px; height:32px; border-radius:7px; background:var(--blue-soft); color:var(--blue); }
.feature:nth-child(2) .feature-icon { background:var(--ok-soft); color:var(--ok); }
.feature:nth-child(3) .feature-icon { background:var(--orange-soft); color:var(--orange); }
.feature > span:last-child { display:flex; min-width:0; flex-direction:column; gap:3px; }
.feature b { color:var(--text); font-size:13.5px; font-weight:650; }
.feature small { color:var(--text-2); font-size:11.5px; line-height:1.5; }
code { font-family:var(--font-mono); font-size:10.8px; }
.auth { margin:19px 3px 0; }
.auth h2 { margin-bottom:7px; }
.note { margin:0; color:var(--text-3); font-size:11.8px; line-height:1.75; }
.error { margin:10px 3px 0; color:var(--danger); font-size:12px; line-height:1.5; }
.primary { display:flex; align-items:center; justify-content:center; gap:7px; width:100%; min-height:44px; margin-top:20px; border:0; border-radius:10px; background:var(--blue); color:#fff; font-size:14px; } .primary:disabled { opacity:.55; }
.spin { animation:spin 1s linear infinite; } @keyframes spin { to { transform:rotate(360deg); } }
</style>
