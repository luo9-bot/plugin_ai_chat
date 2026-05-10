<template>
  <div v-if="!loggedIn" class="login-page">
    <div class="login-box">
      <h2>AI Chat Admin</h2>
      <input v-model="tokenInput" type="password" placeholder="输入管理员 Token" @keydown.enter="doLogin" autofocus />
      <button @click="doLogin">登录</button>
      <div class="err">{{ loginErr }}</div>
    </div>
  </div>
  <div v-else class="app-layout">
    <header>
      <button class="menu-btn" @click="sidebarOpen = !sidebarOpen">
        <span></span>
        <span></span>
        <span></span>
      </button>
      <h1>AI Chat Admin</h1>
      <button class="logout-btn" @click="doLogout">退出</button>
    </header>
    <div class="container">
      <nav :class="{ open: sidebarOpen }" @click="sidebarOpen = false">
        <a v-for="t in tabs" :key="t.id" :class="{ active: currentTab === t.id }" @click="currentTab = t.id">
          {{ t.name }}
        </a>
      </nav>
      <div v-if="sidebarOpen" class="sidebar-overlay" @click="sidebarOpen = false"></div>
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
import ProactiveView from './views/ProactiveView.vue'
import ArchiveView from './views/ArchiveView.vue'
import BackupsView from './views/BackupsView.vue'
import SyncView from './views/SyncView.vue'
import AntiInjectionView from './views/AntiInjectionView.vue'
import ConversationsView from './views/ConversationsView.vue'
import QuotaView from './views/QuotaView.vue'
import ConfigView from './views/ConfigView.vue'

const loggedIn = ref(false)
const tokenInput = ref('')
const loginErr = ref('')
const currentTab = ref('self-thoughts')
const sidebarOpen = ref(false)

const tabs = [
  { id: 'config', name: '配置', comp: ConfigView },
  { id: 'conversations', name: '对话管理', comp: ConversationsView },
  { id: 'quota', name: '配额追踪', comp: QuotaView },
  { id: 'self-thoughts', name: '自我记忆', comp: SelfThoughts },
  { id: 'memory', name: '用户记忆', comp: UserMemory },
  { id: 'working-memory', name: '工作记忆', comp: WorkingMemory },
  { id: 'personality', name: '人格', comp: PersonalityView },
  { id: 'emotion', name: '情绪', comp: EmotionView },
  { id: 'mental-state', name: '心理状态', comp: MentalState },
  { id: 'proactive', name: '主动对话', comp: ProactiveView },
  { id: 'anti-injection', name: '防注入', comp: AntiInjectionView },
  { id: 'archive', name: '归档', comp: ArchiveView },
  { id: 'backups', name: '备份', comp: BackupsView },
  { id: 'sync', name: '同步', comp: SyncView },
]

const tabComponent = computed(() => tabs.find(t => t.id === currentTab.value)?.comp)

async function doLogin() {
  loginErr.value = ''
  const t = tokenInput.value.trim()
  if (!t) return
  try {
    if (await tryLogin(t)) { loggedIn.value = true }
    else loginErr.value = '登录失败'
  } catch { loginErr.value = '网络错误' }
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
  --bg: #f5f5f5; --surface: #fff; --border: #e5e7eb;
  --text: #1f2937; --text-dim: #6b7280; --accent: #6366f1; --accent-light: #eef2ff;
  --accent-hover: #4f46e5; --danger: #ef4444; --danger-light: #fee2e2;
  --success: #10b981; --success-light: #d1fae5;
  --radius: 8px; --shadow: 0 1px 3px rgba(0,0,0,.1);
}
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: var(--bg); color: var(--text); height: 100vh; font-size: 14px; }

/* Login */
.login-page { display: flex; align-items: center; justify-content: center; height: 100vh; background: var(--bg); }
.login-box { display: flex; flex-direction: column; align-items: center; gap: 16px; }
.login-box h2 { font-size: 24px; color: var(--accent); }
.login-box input { background: #fff; border: 1.5px solid var(--border); color: var(--text); padding: 12px 18px; border-radius: var(--radius); width: 280px; font-size: 14px; outline: none; transition: border .2s; }
.login-box input:focus { border-color: var(--accent); }
.login-box button { background: var(--accent); color: #fff; border: none; padding: 12px 32px; border-radius: var(--radius); cursor: pointer; font-size: 14px; font-weight: 500; transition: background .15s; }
.login-box button:hover { background: var(--accent-hover); }
.login-box .err { color: var(--danger); font-size: 13px; min-height: 20px; }

/* Layout */
.app-layout { display: flex; flex-direction: column; height: 100vh; }
header { background: #fff; border-bottom: 1px solid var(--border); padding: 12px 16px; display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; }
header h1 { font-size: 18px; color: var(--accent); font-weight: 700; }

/* Menu button (hamburger) */
.menu-btn { display: none; background: none; border: none; cursor: pointer; padding: 4px; flex-direction: column; gap: 4px; }
.menu-btn span { display: block; width: 20px; height: 2px; background: var(--text); transition: all .2s; }

.logout-btn { background: var(--accent-light); border: 1px solid var(--border); color: var(--accent); padding: 6px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; transition: all .15s; }
.logout-btn:hover { background: var(--accent); color: #fff; }

.container { display: flex; flex: 1; overflow: hidden; position: relative; }

/* Sidebar */
nav { width: 200px; background: #fff; border-right: 1px solid var(--border); padding: 8px 0; overflow-y: auto; flex-shrink: 0; transition: transform .2s; }
nav a { display: block; padding: 10px 20px; color: var(--text-dim); text-decoration: none; font-size: 13px; cursor: pointer; border-left: 3px solid transparent; transition: all .15s; }
nav a:hover { color: var(--accent); background: var(--accent-light); }
nav a.active { color: var(--accent); border-left-color: var(--accent); background: var(--accent-light); font-weight: 500; }

.sidebar-overlay { display: none; }

main { flex: 1; overflow-y: auto; padding: 20px; }

/* Mobile */
@media (max-width: 768px) {
  .menu-btn { display: flex; }

  nav {
    position: fixed; top: 52px; left: 0; bottom: 0;
    width: 240px; z-index: 100;
    transform: translateX(-100%);
    box-shadow: 2px 0 8px rgba(0,0,0,.1);
  }
  nav.open { transform: translateX(0); }

  .sidebar-overlay {
    display: block; position: fixed; top: 52px; left: 0; right: 0; bottom: 0;
    background: rgba(0,0,0,.3); z-index: 99;
  }

  main { padding: 16px; }

  header h1 { font-size: 16px; }
}
</style>
