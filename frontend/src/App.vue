<template>
  <div v-if="!loggedIn" class="login-page">
    <div class="login-box">
      <svg class="login-logo" viewBox="0 0 40 40" fill="none" width="48" height="48">
        <rect x="4" y="4" width="32" height="32" rx="8" fill="url(#lg)"/>
        <path d="M14 20l4 4 8-8" stroke="#fff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
        <defs><linearGradient id="lg" x1="0" y1="0" x2="40" y2="40"><stop stop-color="#4f46e5"/><stop offset="1" stop-color="#6366f1"/></linearGradient></defs>
      </svg>
      <h2>Luo9 AI Chat</h2>
      <p class="login-desc">管理控制台</p>
      <input v-model="tokenInput" type="password" placeholder="输入管理员 Token" @keydown.enter="doLogin" autofocus />
      <button @click="doLogin" :disabled="!tokenInput.trim()">登录</button>
      <div class="err">{{ loginErr }}</div>
      <div class="login-footer" v-if="appVersion"><span>v{{ appVersion }}</span><span v-if="buildTime"> · {{ buildTime }}</span></div>
    </div>
  </div>
  <div v-else class="app-layout">
    <header>
      <button class="menu-btn" @click="sidebarOpen = !sidebarOpen" aria-label="菜单">
        <span></span><span></span><span></span>
      </button>
      <div class="header-left">
        <svg viewBox="0 0 32 32" fill="none" width="24" height="24">
          <rect x="2" y="2" width="28" height="28" rx="6" fill="#4f46e5"/>
          <path d="M11 16l4 4 6-7" stroke="#fff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        <h1>Luo9 AI Chat</h1>
        <span class="version-badge" v-if="appVersion">v{{ appVersion }}</span>
      </div>
      <div class="header-right">
        <button class="btn-icon header-icon" @click="toggleTheme" :title="isDark ? '切换亮色模式' : '切换暗色模式'">
          <svg v-if="isDark" viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2a8 8 0 000 16 6 6 0 010-12 6 6 0 000-4z" stroke="currentColor" stroke-width="1.5"/></svg>
          <svg v-else viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="4" stroke="currentColor" stroke-width="1.5"/><path d="M10 1v2M10 17v2M1 10h2M17 10h2M3.93 3.93l1.41 1.41M14.66 14.66l1.41 1.41M3.93 16.07l1.41-1.41M14.66 5.34l1.41-1.41" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
        </button>
        <button class="btn-icon header-icon" @click="refreshAll" title="刷新全部数据">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M14.5 5.5A6.5 6.5 0 104 10.5M14.5 2v3.5H11M5.5 14.5A6.5 6.5 0 0016 9.5M5.5 18V14.5H9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button class="btn-icon header-icon" @click="doLogout" title="退出登录">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M7 17H4a1 1 0 01-1-1V4a1 1 0 011-1h3M13 14l4-4-4-4M17 10H7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
      </div>
    </header>
    <div class="container">
      <nav :class="{ open: sidebarOpen }">
        <div class="nav-section" v-for="group in navGroups" :key="group.label">
          <div class="nav-section-label">{{ group.label }}</div>
          <a v-for="t in group.items" :key="t.id" :class="{ active: currentTab === t.id }" @click="currentTab = t.id; sidebarOpen = false" :title="t.desc">
            <span class="nav-icon" v-html="t.icon"></span>
            <span class="nav-text">{{ t.name }}</span>
          </a>
        </div>
      </nav>
      <div v-if="sidebarOpen" class="sidebar-overlay" @click="sidebarOpen = false"></div>
      <main>
        <div class="page-header">
          <h2><span v-html="currentTabMeta?.icon" class="page-icon"></span> {{ currentTabMeta?.name || '仪表盘' }}</h2>
          <p class="page-desc" v-if="currentTabMeta?.desc">{{ currentTabMeta.desc }}</p>
        </div>
        <div class="page-body">
          <component :is="tabComponent" :key="currentTab" />
        </div>
      </main>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { getToken, clearToken, tryLogin, api } from './api.js'
import DashboardView from './views/DashboardView.vue'
import ConfigView from './views/ConfigView.vue'
import ConversationsView from './views/ConversationsView.vue'
import QuotaView from './views/QuotaView.vue'
import StickerView from './views/StickerView.vue'
import SelfThoughts from './views/SelfThoughts.vue'
import UserMemory from './views/UserMemory.vue'
import WorkingMemory from './views/WorkingMemory.vue'
import PersonalityView from './views/PersonalityView.vue'
import EmotionView from './views/EmotionView.vue'
import MentalState from './views/MentalState.vue'
import ProactiveView from './views/ProactiveView.vue'
import BlocklistView from './views/BlocklistView.vue'
import AntiInjectionView from './views/AntiInjectionView.vue'
import ArchiveView from './views/ArchiveView.vue'
import BackupsView from './views/BackupsView.vue'
import SyncView from './views/SyncView.vue'
// Note: Planner Monitor and Reply Effect tabs removed - no backend API yet

const I = {
  dashboard: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M2 10a8 8 0 1116 0H2zm2 0a6 6 0 0112 0H4zm2 0a4 4 0 018 0H6zm2 0a2 2 0 014 0H8z" fill="currentColor"/><circle cx="10" cy="10" r="3" fill="currentColor" opacity="0.5"/></svg>',
  planner: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 4a1 1 0 011-1h12a1 1 0 011 1v12a1 1 0 01-1 1H4a1 1 0 01-1-1V4z" stroke="currentColor" stroke-width="1.5"/><path d="M7 7h6M7 10h6M7 13h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'reply-effect': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 10a7 7 0 1114 0H3zm2 0a5 5 0 0110 0H5z" fill="currentColor" opacity="0.3"/><path d="M10 3v14M3 10h14" stroke="currentColor" stroke-width="1.5"/><circle cx="10" cy="10" r="2" fill="currentColor"/></svg>',
  config: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 13a3 3 0 100-6 3 3 0 000 6z" stroke="currentColor" stroke-width="1.5"/><path d="M10 1v2M10 17v2M1 10h2M17 10h2M3.93 3.93l1.41 1.41M14.66 14.66l1.41 1.41M3.93 16.07l1.41-1.41M14.66 5.34l1.41-1.41" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  conversations: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 10a7 7 0 1114 0 7 7 0 01-7 7H3l2-3a7 7 0 01-2-4z" stroke="currentColor" stroke-width="1.5"/><path d="M7 8h6M7 11h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  quota: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M2 10a8 8 0 1116 0H2z" fill="currentColor" opacity="0.15"/><path d="M10 2a8 8 0 100 16 8 8 0 000-16z" stroke="currentColor" stroke-width="1.5"/><path d="M10 6v4l3 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  sticker: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 5a2 2 0 012-2h10a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V5z" stroke="currentColor" stroke-width="1.5"/><circle cx="7.5" cy="8.5" r="1.5" fill="currentColor"/><path d="M5 15l3-4 3 4H5z" fill="currentColor" opacity="0.5"/><path d="M11 13l3-5 3 5H11z" fill="currentColor" opacity="0.5"/></svg>',
  'self-thoughts': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2a7 7 0 00-7 7c0 1.5.5 2.9 1.3 4L3 17l4-1.3A7 7 0 1010 2z" stroke="currentColor" stroke-width="1.5"/><path d="M7 9h6M7 12h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  memory: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M5 3h10a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2z" stroke="currentColor" stroke-width="1.5"/><path d="M8 7h4M8 10h6M8 13h5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'working-memory': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M4 4h12v12H4V4z" stroke="currentColor" stroke-width="1.5"/><path d="M7 7h6M7 10h6M7 13h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  personality: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="6" r="3" stroke="currentColor" stroke-width="1.5"/><path d="M4 17c0-3.3 2.7-6 6-6s6 2.7 6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  emotion: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.5"/><path d="M6.5 8a.5.5 0 100-1 .5.5 0 000 1zM13.5 8a.5.5 0 100-1 .5.5 0 000 1z" fill="currentColor"/><path d="M7 12.5c.8 1 2 1.5 3 1.5s2.2-.5 3-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'mental-state': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2a4 4 0 00-4 4c0 2 1 3 1 4s-.5 2-2 3c1.5 0 3-.5 4-1v2a4 4 0 008-2c0-2-1.5-3-2-5s0-3-2-4a4 4 0 00-3-1z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  proactive: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 3a7 7 0 017 7v3l2 2H3l2-2v-3a7 7 0 017-7z" stroke="currentColor" stroke-width="1.5"/><path d="M8 16c.5 1 1.5 1.5 2 1.5s1.5-.5 2-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  blocklist: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.5"/><path d="M6 6l8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'anti-injection': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2l7 3v5c0 4-3 7-7 8-4-1-7-4-7-8V5l7-3z" stroke="currentColor" stroke-width="1.5"/><path d="M7 10l2 2 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  archive: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 5h14v2H3V5z" fill="currentColor" opacity="0.2"/><path d="M4 7h12v10H4V7z" stroke="currentColor" stroke-width="1.5"/><path d="M3 4a1 1 0 011-1h12a1 1 0 011 1v1H3V4z" stroke="currentColor" stroke-width="1.5"/></svg>',
  backups: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M4 4h12v12H4V4z" stroke="currentColor" stroke-width="1.5"/><path d="M7 10l2 2 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  sync: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M14.5 5.5A6.5 6.5 0 104 10.5M14.5 2v3.5H11M5.5 14.5A6.5 6.5 0 0016 9.5M5.5 18V14.5H9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
}

const loggedIn = ref(false)
const tokenInput = ref('')
const loginErr = ref('')
const currentTab = ref('dashboard')
const sidebarOpen = ref(false)
const appVersion = ref('')
const buildTime = ref('')

const tabs = [
  { id: 'dashboard', name: '仪表盘', icon: I.dashboard, desc: '系统总览与关键指标', comp: DashboardView },
  { id: 'config', name: '配置', icon: I.config, desc: 'Bot 与 AI 参数', comp: ConfigView },
  { id: 'conversations', name: '对话管理', icon: I.conversations, desc: '活跃会话控制', comp: ConversationsView },
  { id: 'quota', name: '配额', icon: I.quota, desc: '回复配额追踪', comp: QuotaView },
  { id: 'sticker', name: '表情包', icon: I.sticker, desc: '表情管理', comp: StickerView },
  { id: 'self-thoughts', name: '自我记忆', icon: I['self-thoughts'], desc: 'Bot 内心想法', comp: SelfThoughts },
  { id: 'memory', name: '用户记忆', icon: I.memory, desc: '长期记忆', comp: UserMemory },
  { id: 'working-memory', name: '工作记忆', icon: I['working-memory'], desc: '短期工作记忆', comp: WorkingMemory },
  { id: 'personality', name: '人格', icon: I.personality, desc: '人设与快照', comp: PersonalityView },
  { id: 'emotion', name: '情绪', icon: I.emotion, desc: '情绪状态', comp: EmotionView },
  { id: 'mental-state', name: '心理状态', icon: I['mental-state'], desc: 'Bot 心理', comp: MentalState },
  { id: 'proactive', name: '主动对话', icon: I.proactive, desc: '主动消息', comp: ProactiveView },
  { id: 'blocklist', name: '黑名单', icon: I.blocklist, desc: '用户管理', comp: BlocklistView },
  { id: 'anti-injection', name: '防注入', icon: I['anti-injection'], desc: '安全防护', comp: AntiInjectionView },
  { id: 'archive', name: '归档', icon: I.archive, desc: '数据归档', comp: ArchiveView },
  { id: 'backups', name: '备份', icon: I.backups, desc: '数据备份', comp: BackupsView },
  { id: 'sync', name: '同步', icon: I.sync, desc: '远程同步', comp: SyncView },
]

const navGroups = computed(() => [
  { label: '概览', items: tabs.slice(0, 1) },
  { label: '管理', items: tabs.slice(1, 5) },
  { label: '数据', items: tabs.slice(5, 8) },
  { label: '状态', items: tabs.slice(8, 12) },
  { label: '系统', items: tabs.slice(12) },
])

const currentTabMeta = computed(() => tabs.find(t => t.id === currentTab.value))
const tabComponent = computed(() => tabs.find(t => t.id === currentTab.value)?.comp)

// ── Theme ──
const isDark = ref(false)

function applyTheme(dark) {
  isDark.value = dark
  document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
  localStorage.setItem('ai-chat-theme', dark ? 'dark' : 'light')
}

function toggleTheme() {
  applyTheme(!isDark.value)
}

function initTheme() {
  const saved = localStorage.getItem('ai-chat-theme')
  if (saved === 'dark' || saved === 'light') {
    applyTheme(saved === 'dark')
  } else {
    applyTheme(window.matchMedia('(prefers-color-scheme: dark)').matches)
  }
}

async function doLogin() {
  loginErr.value = ''
  const t = tokenInput.value.trim()
  if (!t) return
  try {
    if (await tryLogin(t)) { loggedIn.value = true }
    else loginErr.value = 'Token 验证失败'
  } catch { loginErr.value = '网络错误' }
}

function doLogout() { clearToken(); loggedIn.value = false }
function refreshAll() { window.dispatchEvent(new CustomEvent('refresh-all')) }

onMounted(async () => {
  initTheme()
  if (getToken()) {
    try {
      await fetch('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: getToken() }) })
      loggedIn.value = true
    } catch { clearToken() }
  }
  try {
    const v = await api('/api/version')
    appVersion.value = v.version || ''
    buildTime.value = v.build_time || ''
  } catch {}
})
</script>

<style>
/* ── Design System ── */
*, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }

:root {
  /* Light: 暖白粉系 */
  --bg: #fdf2f8;
  --bg-alt: #fce7f3;
  --surface: #ffffff;
  --surface-2: #fff5f9;
  --surface-hover: #fdf2f8;
  --border: #fbcfe8;
  --border-light: #fce7f3;
  --text: #831843;
  --text-2: #9d174d;
  --text-3: #db2777;
  --primary: #ec4899;
  --primary-light: #f472b6;
  --primary-bg: #fdf2f8;
  --primary-hover: #db2777;
  --success: #10b981;
  --success-bg: #ecfdf5;
  --warning: #f59e0b;
  --warning-bg: #fffbeb;
  --danger: #ef4444;
  --danger-bg: #fef2f2;
  --info: #6366f1;
  --info-bg: #eef2ff;
  --radius: 8px;
  --radius-lg: 12px;
  --shadow-sm: 0 1px 2px rgba(236,72,153,0.08);
  --shadow: 0 2px 8px rgba(236,72,153,0.12);
  --shadow-lg: 0 8px 32px rgba(236,72,153,0.15);
  --header-h: 52px;
  --sidebar-w: 200px;
  --transition: 0.15s ease;
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    /* Dark: VS Code Dark+ 风格 */
    --bg: #1e1e1e;
    --bg-alt: #252526;
    --surface: #2d2d2d;
    --surface-2: #333333;
    --surface-hover: #3c3c3c;
    --border: #404040;
    --border-light: #383838;
    --text: #d4d4d4;
    --text-2: #9d9d9d;
    --text-3: #707070;
    --primary: #c586c0;
    --primary-light: #d7a8d4;
    --primary-bg: #3a2d3a;
    --primary-hover: #b07aa8;
    --success: #4ec9b0;
    --success-bg: #1a3a32;
    --warning: #dcdcaa;
    --warning-bg: #3a3520;
    --danger: #f14c4c;
    --danger-bg: #3a1a1a;
    --info: #569cd6;
    --info-bg: #1a2a3a;
    --shadow-sm: 0 1px 2px rgba(0,0,0,0.3);
    --shadow: 0 2px 8px rgba(0,0,0,0.4);
    --shadow-lg: 0 8px 32px rgba(0,0,0,0.5);
  }
}

[data-theme="dark"] {
  --bg: #1e1e1e;
  --bg-alt: #252526;
  --surface: #2d2d2d;
  --surface-2: #333333;
  --surface-hover: #3c3c3c;
  --border: #404040;
  --border-light: #383838;
  --text: #d4d4d4;
  --text-2: #9d9d9d;
  --text-3: #707070;
  --primary: #c586c0;
  --primary-light: #d7a8d4;
  --primary-bg: #3a2d3a;
  --primary-hover: #b07aa8;
  --success: #4ec9b0;
  --success-bg: #1a3a32;
  --warning: #dcdcaa;
  --warning-bg: #3a3520;
  --danger: #f14c4c;
  --danger-bg: #3a1a1a;
  --info: #569cd6;
  --info-bg: #1a2a3a;
  --shadow-sm: 0 1px 2px rgba(0,0,0,0.3);
  --shadow: 0 2px 8px rgba(0,0,0,0.4);
  --shadow-lg: 0 8px 32px rgba(0,0,0,0.5);
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'SF Pro', 'Noto Sans SC', sans-serif;
  background: var(--bg); color: var(--text); height: 100vh;
  font-size: 14px; line-height: 1.5; -webkit-font-smoothing: antialiased;
}

/* ── Login ── */
.login-page {
  display: flex; align-items: center; justify-content: center;
  min-height: 100vh;
  background: linear-gradient(135deg, #eef2ff 0%, #f8fafc 50%, #f0fdf4 100%);
}
.login-box {
  display: flex; flex-direction: column; align-items: center; gap: 16px;
  background: var(--surface); padding: 40px 36px 32px;
  border-radius: var(--radius-lg); box-shadow: var(--shadow-lg);
  width: 340px; max-width: 90vw;
}
.login-box h2 { font-size: 20px; font-weight: 700; color: var(--text); }
.login-desc { font-size: 13px; color: var(--text-3); margin-top: -8px; }
.login-box input {
  width: 100%; background: var(--surface-2); border: 1.5px solid var(--border);
  color: var(--text); padding: 10px 14px; border-radius: var(--radius);
  font-size: 14px; outline: none; transition: border-color var(--transition);
}
.login-box input:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-bg); }
.login-box button {
  width: 100%; background: var(--primary); color: #fff; border: none;
  padding: 10px 20px; border-radius: var(--radius); cursor: pointer;
  font-size: 14px; font-weight: 600; transition: background var(--transition);
}
.login-box button:hover:not(:disabled) { background: var(--primary-hover); }
.login-box button:disabled { opacity: 0.5; cursor: not-allowed; }
.login-box .err { color: var(--danger); font-size: 13px; min-height: 20px; }
.login-footer { font-size: 11px; color: var(--text-3); margin-top: 4px; }

/* ── Layout ── */
.app-layout { display: flex; flex-direction: column; height: 100vh; }
header {
  height: var(--header-h); background: var(--surface);
  border-bottom: 1px solid var(--border);
  padding: 0 12px; display: flex; align-items: center;
  justify-content: space-between; flex-shrink: 0; z-index: 50;
}
.header-left { display: flex; align-items: center; gap: 10px; }
header h1 { font-size: 15px; font-weight: 700; color: var(--text); }
.version-badge { font-size: 10px; padding: 1px 7px; border-radius: 8px; background: var(--primary-bg); color: var(--primary); font-weight: 600; }
.header-right { display: flex; align-items: center; gap: 2px; }
.header-icon { color: var(--text-3); }
.header-icon:hover { color: var(--text); background: var(--surface-hover); }
.menu-btn { display: none; background: none; border: none; cursor: pointer; padding: 4px; flex-direction: column; gap: 4px; }
.menu-btn span { display: block; width: 18px; height: 2px; background: var(--text-3); border-radius: 1px; }

.container { display: flex; flex: 1; overflow: hidden; }

/* ── Sidebar ── */
nav {
  width: var(--sidebar-w); background: var(--surface);
  border-right: 1px solid var(--border);
  padding: 8px 0; overflow-y: auto; flex-shrink: 0;
  transition: transform var(--transition);
}
.nav-section { margin-bottom: 4px; }
.nav-section-label {
  padding: 6px 16px 3px; font-size: 10px; font-weight: 600;
  color: var(--text-3); letter-spacing: 0.3px;
}
nav a {
  display: flex; align-items: center; gap: 8px;
  padding: 6px 16px; color: var(--text-2); font-size: 13px;
  cursor: pointer; transition: all var(--transition);
  border-left: 2px solid transparent;
}
nav a:hover { color: var(--text); background: var(--surface-hover); }
nav a.active { color: var(--primary); border-left-color: var(--primary); background: var(--primary-bg); font-weight: 500; }
.nav-icon { width: 18px; height: 18px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
.nav-icon :deep(svg) { display: block; }
.sidebar-overlay { display: none; }

/* ── Main ── */
main { flex: 1; overflow-y: auto; padding: 24px; background: var(--bg); }
.page-header { margin-bottom: 20px; }
.page-header h2 { font-size: 20px; font-weight: 700; color: var(--text); display: flex; align-items: center; gap: 8px; }
.page-icon { display: inline-flex; align-items: center; }
.page-icon :deep(svg) { display: block; }
.page-desc { font-size: 13px; color: var(--text-3); margin-top: 4px; }
.page-body { max-width: 1200px; }

/* ── Common ── */
.section {
  background: var(--surface); border: 1px solid var(--border);
  border-radius: var(--radius); padding: 20px; margin-bottom: 12px;
}
.section h3 { font-size: 14px; font-weight: 600; margin-bottom: 14px; color: var(--text); display: flex; align-items: center; gap: 6px; }
.section .desc { font-size: 12px; color: var(--text-3); margin: -10px 0 14px; }
.empty { text-align: center; padding: 40px 20px; color: var(--text-3); font-size: 13px; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }

.badge {
  display: inline-flex; align-items: center; font-size: 11px; font-weight: 500;
  padding: 2px 8px; border-radius: 6px; gap: 4px;
}
.badge-primary { background: var(--primary-bg); color: var(--primary); }
.badge-success { background: var(--success-bg); color: var(--success); }
.badge-warning { background: var(--warning-bg); color: var(--warning); }
.badge-danger { background: var(--danger-bg); color: var(--danger); }
.badge-info { background: var(--info-bg); color: var(--info); }

.btn {
  display: inline-flex; align-items: center; gap: 4px;
  border: none; padding: 7px 14px; border-radius: 6px;
  cursor: pointer; font-size: 13px; font-weight: 500;
  transition: all var(--transition); white-space: nowrap;
}
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn-primary { background: var(--primary); color: #fff; }
.btn-primary:hover:not(:disabled) { background: var(--primary-hover); }
.btn-success { background: var(--success); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-warning { background: var(--warning); color: #1a1d23; }
.btn-outline { background: transparent; border: 1px solid var(--border); color: var(--text-2); }
.btn-outline:hover { border-color: var(--primary); color: var(--primary); }
.btn-ghost { background: transparent; color: var(--text-3); }
.btn-ghost:hover { background: var(--surface-hover); color: var(--text); }
.btn-sm { padding: 5px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.btn-icon { width: 32px; height: 32px; padding: 0; justify-content: center; border-radius: 6px; background: none; border: none; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; transition: background var(--transition); }

label { font-size: 12px; font-weight: 600; color: var(--text-2); display: block; margin-bottom: 4px; }
.hint { font-size: 11px; color: var(--text-3); }
input, textarea, select {
  background: var(--surface-2); border: 1px solid var(--border); color: var(--text);
  padding: 8px 12px; border-radius: 6px; font-size: 13px; outline: none;
  width: 100%; transition: border-color var(--transition); font-family: inherit;
}
input:focus, textarea:focus, select:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-bg); }
textarea { resize: vertical; min-height: 60px; }
select { cursor: pointer; }

table { width: 100%; border-collapse: collapse; font-size: 13px; }
table th { text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600; color: var(--text-3); border-bottom: 1px solid var(--border); }
table td { padding: 8px 12px; border-bottom: 1px solid var(--border-light); }
table tr:hover td { background: var(--surface-hover); }

.stat-row { display: flex; gap: 10px; margin-bottom: 16px; flex-wrap: wrap; }
.stat-card {
  flex: 1; min-width: 150px; background: var(--surface);
  border: 1px solid var(--border); border-radius: var(--radius); padding: 16px 18px;
}
.stat-card:hover { box-shadow: var(--shadow); }
.stat-card .stat-num { font-size: 26px; font-weight: 700; color: var(--primary); line-height: 1.2; }
.stat-card .stat-label { font-size: 12px; color: var(--text-3); margin-top: 2px; }
.stat-card .stat-sub { font-size: 11px; color: var(--text-3); margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--border-light); }

.toolbar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 14px; }
.toolbar .search-input { width: 180px; }
.toolbar-right { margin-left: auto; display: flex; align-items: center; gap: 10px; }

.filter-tabs { display: flex; gap: 2px; background: var(--surface-2); border-radius: 6px; padding: 2px; }
.filter-tabs .tab {
  background: none; border: none; padding: 5px 12px; border-radius: 5px;
  cursor: pointer; font-size: 12px; font-weight: 500;
  color: var(--text-3); transition: all var(--transition); white-space: nowrap;
}
.filter-tabs .tab:hover { color: var(--text); }
.filter-tabs .tab.active { background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); }
.filter-tabs .tab .count { display: inline-block; margin-left: 4px; font-size: 10px; padding: 0 5px; border-radius: 6px; background: var(--primary-bg); color: var(--primary); }

.progress-bar { height: 4px; background: var(--surface-2); border-radius: 2px; overflow: hidden; }
.progress-fill { height: 100%; border-radius: 2px; transition: width 0.4s; }

.modal-overlay {
  position: fixed; inset: 0; background: rgba(0,0,0,0.3); backdrop-filter: blur(2px);
  display: flex; align-items: center; justify-content: center; z-index: 100;
}
.modal {
  background: var(--surface); border-radius: var(--radius-lg);
  padding: 24px; width: 460px; max-width: 90vw;
  max-height: 80vh; overflow-y: auto;
  box-shadow: var(--shadow-lg);
}
.modal h3 { font-size: 15px; font-weight: 700; margin-bottom: 14px; color: var(--text); }
.modal .modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }

/* Scrollbar */
::-webkit-scrollbar { width: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }

/* Mobile */
@media (max-width: 768px) {
  .menu-btn { display: flex; }
  nav { position: fixed; top: var(--header-h); left: 0; bottom: 0; width: 240px; z-index: 100; transform: translateX(-100%); box-shadow: 4px 0 12px rgba(0,0,0,0.1); }
  nav.open { transform: translateX(0); }
  .sidebar-overlay { display: block; position: fixed; top: var(--header-h); left: 0; right: 0; bottom: 0; background: rgba(0,0,0,0.2); z-index: 99; }
  main { padding: 16px; }
  .grid { grid-template-columns: 1fr; }
}
</style>
