<template>
  <div v-if="!loggedIn" class="login-page">
    <div class="login-box">
      <h2>🐱 AI Chat Admin</h2>
      <input v-model="tokenInput" type="password" placeholder="🔑 输入管理员 Token" @keydown.enter="doLogin" autofocus />
      <button @click="doLogin">登录</button>
      <div class="err">{{ loginErr }}</div>
    </div>
  </div>
  <div v-else class="app-layout">
    <header>
      <h1>🐱 AI Chat Admin</h1>
      <button @click="doLogout">退出</button>
    </header>
    <div class="container">
      <nav>
        <a v-for="t in tabs" :key="t.id" :class="{ active: currentTab === t.id }" @click="currentTab = t.id">{{ t.name }}</a>
      </nav>
      <main>
        <component :is="tabComponent" />
      </main>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { getToken, clearToken, tryLogin } from './api.js'
import SelfThoughts from './views/SelfThoughts.vue'
import UserMemory from './views/UserMemory.vue'
import WorkingMemory from './views/WorkingMemory.vue'
import PersonalityView from './views/PersonalityView.vue'
import EmotionView from './views/EmotionView.vue'
import MentalState from './views/MentalState.vue'
import BlocklistView from './views/BlocklistView.vue'
import ProactiveView from './views/ProactiveView.vue'
import ArchiveView from './views/ArchiveView.vue'
import BackupsView from './views/BackupsView.vue'
import SyncView from './views/SyncView.vue'

const loggedIn = ref(false)
const tokenInput = ref('')
const loginErr = ref('')
const currentTab = ref('self-thoughts')

const tabs = [
  { id: 'self-thoughts', name: '🧠 自我记忆', comp: SelfThoughts },
  { id: 'memory', name: '📦 用户记忆', comp: UserMemory },
  { id: 'working-memory', name: '💬 工作记忆', comp: WorkingMemory },
  { id: 'personality', name: '🎭 人格', comp: PersonalityView },
  { id: 'emotion', name: '😊 情绪', comp: EmotionView },
  { id: 'mental-state', name: '🧬 心理状态', comp: MentalState },
  { id: 'blocklist', name: '🚫 黑名单', comp: BlocklistView },
  { id: 'proactive', name: '📢 主动对话', comp: ProactiveView },
  { id: 'archive', name: '📦 归档', comp: ArchiveView },
  { id: 'backups', name: '💾 备份', comp: BackupsView },
  { id: 'sync', name: '🔄 同步', comp: SyncView },
]

const tabComponent = computed(() => tabs.find(t => t.id === currentTab.value)?.comp)

async function doLogin() {
  loginErr.value = ''
  const t = tokenInput.value.trim()
  if (!t) return
  try {
    if (await tryLogin(t)) { loggedIn.value = true }
    else loginErr.value = '😿 登录失败'
  } catch { loginErr.value = '😿 网络错误' }
}

function doLogout() {
  clearToken()
  loggedIn.value = false
}

onMounted(async () => {
  if (getToken()) {
    try {
      await import('./api.js').then(m => m.api('/api/login', { method: 'POST', body: JSON.stringify({ token: getToken() }) }))
      loggedIn.value = true }
    catch { clearToken() }
  }
})
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
:root {
  --bg: #fef6f9; --surface: #fff; --surface2: #fdf2f8; --border: #f9a8d4;
  --text: #4a3548; --text-dim: #9b8a99; --accent: #ec4899; --accent-light: #fce7f3;
  --accent-hover: #db2777; --danger: #ef4444; --danger-light: #fee2e2;
  --success: #10b981; --success-light: #d1fae5; --warning: #f59e0b;
  --purple: #a855f7; --purple-light: #f3e8ff; --blue: #6366f1; --blue-light: #e0e7ff;
  --radius: 12px; --shadow: 0 2px 8px rgba(236,72,153,.1);
}
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: var(--bg); color: var(--text); height: 100vh; font-size: 14px; }
.login-page { display: flex; align-items: center; justify-content: center; height: 100vh; background: linear-gradient(135deg, #fce7f3, #f3e8ff, #e0e7ff); }
.login-box { display: flex; flex-direction: column; align-items: center; gap: 16px; }
.login-box h2 { font-size: 24px; color: var(--accent); }
.login-box input { background: #fff; border: 2px solid var(--border); color: var(--text); padding: 12px 18px; border-radius: var(--radius); width: 300px; font-size: 14px; outline: none; transition: border .2s; }
.login-box input:focus { border-color: var(--accent); }
.login-box button { background: linear-gradient(135deg, var(--accent), var(--purple)); color: #fff; border: none; padding: 12px 32px; border-radius: var(--radius); cursor: pointer; font-size: 14px; font-weight: 500; transition: transform .15s, box-shadow .15s; }
.login-box button:hover { transform: translateY(-1px); box-shadow: 0 4px 12px rgba(236,72,153,.3); }
.login-box .err { color: var(--danger); font-size: 13px; min-height: 20px; }
.app-layout { display: flex; flex-direction: column; height: 100vh; }
header { background: #fff; border-bottom: 2px solid var(--accent-light); padding: 12px 24px; display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; }
header h1 { font-size: 18px; background: linear-gradient(135deg, var(--accent), var(--purple)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; font-weight: 700; }
header button { background: var(--accent-light); border: 1px solid var(--border); color: var(--accent); padding: 6px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; transition: all .15s; }
header button:hover { background: var(--accent); color: #fff; }
.container { display: flex; flex: 1; overflow: hidden; }
nav { width: 200px; background: #fff; border-right: 2px solid var(--accent-light); padding: 12px 0; overflow-y: auto; flex-shrink: 0; }
nav a { display: block; padding: 10px 20px; color: var(--text-dim); text-decoration: none; font-size: 13px; cursor: pointer; border-left: 3px solid transparent; transition: all .15s; }
nav a:hover { color: var(--accent); background: var(--accent-light); }
nav a.active { color: var(--accent); border-left-color: var(--accent); background: var(--accent-light); font-weight: 500; }
main { flex: 1; overflow-y: auto; padding: 24px; }
</style>
