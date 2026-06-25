<template>
  <div v-if="!loggedIn" class="login-page">
    <div class="login-card">
      <div class="login-icon">
        <svg viewBox="0 0 40 40" fill="none" width="44" height="44">
          <rect x="2" y="2" width="36" height="36" rx="10" fill="var(--primary)"/>
          <path d="M14 20l4 4 8-8" stroke="#fff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>
      <h1 class="login-title">Luo9 AI Chat</h1>
      <p class="login-desc">管理控制台</p>
      <div class="login-input-group">
        <input v-model="tokenInput" type="password" placeholder="输入管理员 Token" @keydown.enter="doLogin" autofocus />
        <button @click="doLogin" :disabled="!tokenInput.trim()" class="login-btn">
          <span>登录</span>
          <svg viewBox="0 0 20 20" fill="none" width="16" height="16"><path d="M4 10h12M12 6l4 4-4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
      </div>
      <div class="login-err" v-if="loginErr">{{ loginErr }}</div>
      <div class="login-footer" v-if="appVersion">
        <span>v{{ appVersion }}</span>
        <span class="sep">·</span>
        <span>{{ buildTime }}</span>
      </div>
    </div>
  </div>
  <div v-else class="app">
    <header class="app-header">
      <div class="header-left">
        <button class="menu-btn" @click="sidebarOpen = !sidebarOpen" aria-label="菜单">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 5h14M3 10h14M3 15h14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
        </button>
        <svg viewBox="0 0 32 32" fill="none" width="26" height="26">
          <rect x="2" y="2" width="28" height="28" rx="8" fill="var(--primary)"/>
          <path d="M11 16l4 4 6-7" stroke="#fff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        <h1>Luo9 AI Chat</h1>
        <span class="badge" v-if="appVersion">v{{ appVersion }}</span>
      </div>
      <div class="header-right">
        <button class="icon-btn" @click="toggleTheme" :title="isDark ? '切换亮色模式' : '切换暗色模式'">
          <svg v-if="isDark" viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="4" stroke="currentColor" stroke-width="1.5"/><path d="M10 2v2M10 16v2M2 10h2M16 10h2M4.22 4.22l1.42 1.42M14.36 14.36l1.42 1.42M4.22 15.78l1.42-1.42M14.36 5.64l1.42-1.42" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          <svg v-else viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2a8 8 0 100 16 6 6 0 010-12 6 6 0 000-4z" fill="currentColor"/></svg>
        </button>
        <button class="icon-btn" @click="refreshAll" title="刷新">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M14.5 5.5A6.5 6.5 0 104 10.5M14.5 2v3.5H11M5.5 14.5A6.5 6.5 0 0016 9.5M5.5 18V14.5H9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button class="icon-btn" @click="doLogout" title="退出">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M7 17H4a1 1 0 01-1-1V4a1 1 0 011-1h3M13 14l4-4-4-4M17 10H7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
      </div>
    </header>
    <div class="app-body">
      <nav :class="{ open: sidebarOpen }" class="sidebar">
        <div class="nav-group" v-for="group in navGroups" :key="group.label">
          <div class="nav-group-label">{{ group.label }}</div>
          <a v-for="t in group.items" :key="t.id"
             :class="{ active: currentTab === t.id }"
             @click="currentTab = t.id; sidebarOpen = false"
             :title="t.desc">
            <span class="nav-icon" v-html="t.icon"></span>
            <span class="nav-text">{{ t.name }}</span>
          </a>
        </div>
      </nav>
      <div v-if="sidebarOpen" class="overlay" @click="sidebarOpen = false"></div>
      <main class="main-content">
        <div class="page-header animate-fade" :key="currentTab">
          <div class="page-title-row">
            <span class="page-icon" v-html="currentTabMeta?.icon"></span>
            <h2>{{ currentTabMeta?.name || '仪表盘' }}</h2>
          </div>
          <p class="page-desc" v-if="currentTabMeta?.desc">{{ currentTabMeta.desc }}</p>
        </div>
        <div class="page-content animate-slide" :key="currentTab + '_content'">
          <component :is="tabComponent" />
        </div>
      </main>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { getToken, setToken, clearToken, tryLogin, api } from './api.js'
import DashboardView from './views/DashboardView.vue'
import ConfigView from './views/ConfigView.vue'
import ConversationsView from './views/ConversationsView.vue'
import QuotaView from './views/QuotaView.vue'
import StickerView from './views/StickerView.vue'
import SelfThoughts from './views/SelfThoughts.vue'
import UserMemory from './views/UserMemory.vue'
import WorkingMemory from './views/WorkingMemory.vue'
import EmotionView from './views/EmotionView.vue'
import MentalState from './views/MentalState.vue'
import ProactiveView from './views/ProactiveView.vue'
import BlocklistView from './views/BlocklistView.vue'
import AntiInjectionView from './views/AntiInjectionView.vue'
import ArchiveView from './views/ArchiveView.vue'
import BackupsView from './views/BackupsView.vue'
import SyncView from './views/SyncView.vue'
import MemoryOpsLog from './views/MemoryOpsLog.vue'
import ScheduleView from './views/ScheduleView.vue'
import AnalyticsView from './views/AnalyticsView.vue'
import HumanityView from './views/HumanityView.vue'

const I = {
  dashboard: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="3" y="3" width="6" height="6" rx="1.5" stroke="currentColor" stroke-width="1.5"/><rect x="11" y="3" width="6" height="6" rx="1.5" stroke="currentColor" stroke-width="1.5"/><rect x="3" y="11" width="6" height="6" rx="1.5" stroke="currentColor" stroke-width="1.5"/><rect x="11" y="11" width="6" height="6" rx="1.5" stroke="currentColor" stroke-width="1.5"/></svg>',
  analytics: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 17V7l4 4 3-6 3 3 4-5v14H3z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/></svg>',
  schedule: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="3" y="4" width="14" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M3 8h14M7 1v3M13 1v3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  config: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="3" stroke="currentColor" stroke-width="1.5"/><path d="M10 1v2M10 17v2M1 10h2M17 10h2M4.22 4.22l1.42 1.42M14.36 14.36l1.42 1.42M4.22 15.78l1.42-1.42M14.36 5.64l1.42-1.42" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  conversations: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 10a7 7 0 1114 0 7 7 0 01-7 7H3l2-3a7 7 0 01-2-4z" stroke="currentColor" stroke-width="1.5"/><path d="M7 8h6M7 11h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  quota: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.5"/><path d="M10 6v4l3 2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  sticker: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="3" y="3" width="14" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/><circle cx="7.5" cy="8.5" r="1.5" fill="currentColor"/><path d="M5 15l3-4 3 4H5z" fill="currentColor" opacity="0.5"/><path d="M11 13l3-5 3 5H11z" fill="currentColor" opacity="0.5"/></svg>',
  'self-thoughts': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2a7 7 0 00-7 7c0 1.5.5 2.9 1.3 4L3 17l4-1.3A7 7 0 1010 2z" stroke="currentColor" stroke-width="1.5"/></svg>',
  memory: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="4" y="3" width="12" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M7 7h6M7 10h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'working-memory': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="3" y="3" width="14" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M7 7h6M7 10h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  emotion: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.5"/><circle cx="7.5" cy="8.5" r="1" fill="currentColor"/><circle cx="12.5" cy="8.5" r="1" fill="currentColor"/><path d="M7 12.5c.8 1 2 1.5 3 1.5s2.2-.5 3-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'mental-state': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2a4 4 0 00-4 4c0 2 1 3 1 4s-.5 2-2 3c1.5 0 3-.5 4-1v2a4 4 0 008-2c0-2-1.5-3-2-5s0-3-2-4a4 4 0 00-3-1z" stroke="currentColor" stroke-width="1.5"/></svg>',
  proactive: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 3a7 7 0 017 7v3l2 2H3l2-2v-3a7 7 0 017-7z" stroke="currentColor" stroke-width="1.5"/><path d="M8 17h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  humanity: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 3a7 7 0 017 7c0 2.5-1 4.5-2.5 5.5S10 17 10 17s-3.5-.5-4.5-1.5S3 12.5 3 10a7 7 0 017-7z" stroke="currentColor" stroke-width="1.5"/><circle cx="8" cy="9" r="1" fill="currentColor"/><circle cx="12" cy="9" r="1" fill="currentColor"/><path d="M7 12.5c.8 1 2 1.5 3 1.5s2.2-.5 3-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  blocklist: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.5"/><path d="M6 6l8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  'anti-injection': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2l7 3v5c0 4-3 7-7 8-4-1-7-4-7-8V5l7-3z" stroke="currentColor" stroke-width="1.5"/><path d="M7 10l2 2 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  archive: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="3" y="6" width="14" height="11" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M2 4a1 1 0 011-1h14a1 1 0 011 1v2H2V4z" stroke="currentColor" stroke-width="1.5"/><path d="M8 10h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  backups: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 3a5 5 0 00-4.5 2.8A4 4 0 003 10a4 4 0 004 4h1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M10 3a5 5 0 014.5 2.8A4 4 0 0117 10a4 4 0 01-4 4h-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M10 10v5M7 12.5l3-2.5 3 2.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  sync: '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M14.5 5.5A6.5 6.5 0 104 10.5M14.5 2v3.5H11M5.5 14.5A6.5 6.5 0 0016 9.5M5.5 18V14.5H9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  'memory-ops-log': '<svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 3h14v14H3z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M7 7h6M7 10h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="14" cy="14" r="2" fill="currentColor" opacity="0.6"/></svg>',
}

const loggedIn = ref(false)
const tokenInput = ref('')
const loginErr = ref('')
const currentTab = ref('dashboard')
const sidebarOpen = ref(false)
const appVersion = ref('')
const buildTime = ref('')
const isDark = ref(false)

const tabs = [
  { id: 'dashboard', name: '仪表盘', icon: I.dashboard, desc: '系统总览与关键指标', comp: DashboardView },
  { id: 'analytics', name: 'Token 分析', icon: I.analytics, desc: 'API 用量与 Prompt 统计', comp: AnalyticsView },
  { id: 'schedule', name: '日程计划', icon: I.schedule, desc: '周/月计划管理', comp: ScheduleView },
  { id: 'config', name: '配置', icon: I.config, desc: 'Bot 与 AI 参数', comp: ConfigView },
  { id: 'conversations', name: '对话管理', icon: I.conversations, desc: '活跃会话控制', comp: ConversationsView },
  { id: 'quota', name: '配额', icon: I.quota, desc: '回复配额', comp: QuotaView },
  { id: 'sticker', name: '表情包', icon: I.sticker, desc: '表情管理', comp: StickerView },
  { id: 'self-thoughts', name: '自我记忆', icon: I['self-thoughts'], desc: '内心想法', comp: SelfThoughts },
  { id: 'memory', name: '用户记忆', icon: I.memory, desc: '长期记忆', comp: UserMemory },
  { id: 'working-memory', name: '工作记忆', icon: I['working-memory'], desc: '短期工作记忆', comp: WorkingMemory },
  { id: 'memory-ops-log', name: '内存监视', icon: I['memory-ops-log'], desc: '内存操作日志', comp: MemoryOpsLog },
  { id: 'emotion', name: '情绪', icon: I.emotion, desc: '情绪状态', comp: EmotionView },
  { id: 'mental-state', name: '心理状态', icon: I['mental-state'], desc: 'Bot 心理', comp: MentalState },
  { id: 'proactive', name: '主动对话', icon: I.proactive, desc: '主动消息', comp: ProactiveView },
  { id: 'humanity', name: '人性化', icon: I.humanity, desc: '人性化状态监控', comp: HumanityView },
  { id: 'blocklist', name: '黑名单', icon: I.blocklist, desc: '用户管理', comp: BlocklistView },
  { id: 'anti-injection', name: '防注入', icon: I['anti-injection'], desc: '安全防护', comp: AntiInjectionView },
  { id: 'archive', name: '归档', icon: I.archive, desc: '数据归档', comp: ArchiveView },
  { id: 'backups', name: '备份', icon: I.backups, desc: '数据备份', comp: BackupsView },
  { id: 'sync', name: '同步', icon: I.sync, desc: '远程同步', comp: SyncView },
]

const navGroups = computed(() => [
  { label: '概览', items: tabs.slice(0, 2) },
  { label: '管理', items: tabs.slice(2, 6) },
  { label: '数据', items: tabs.slice(6, 11) },
  { label: '状态', items: tabs.slice(11, 15) },
  { label: '系统', items: tabs.slice(15) },
])

const currentTabMeta = computed(() => tabs.find(t => t.id === currentTab.value))
const tabComponent = computed(() => tabs.find(t => t.id === currentTab.value)?.comp)

function applyTheme(dark) {
  isDark.value = dark
  document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
  localStorage.setItem('ai-chat-theme', dark ? 'dark' : 'light')
}
function toggleTheme() { applyTheme(!isDark.value) }
function initTheme() {
  const saved = localStorage.getItem('ai-chat-theme')
  if (saved === 'dark' || saved === 'light') applyTheme(saved === 'dark')
  else applyTheme(window.matchMedia('(prefers-color-scheme: dark)').matches)
}

async function doLogin() {
  loginErr.value = ''
  const t = tokenInput.value.trim()
  if (!t) return
  try { if (await tryLogin(t)) loggedIn.value = true; else loginErr.value = 'Token 验证失败' }
  catch { loginErr.value = '网络错误' }
}
function doLogout() { clearToken(); loggedIn.value = false }
function refreshAll() { window.dispatchEvent(new CustomEvent('refresh-all')) }

onMounted(async () => {
  initTheme()
  const t = getToken()
  if (t) { try { if (await tryLogin(t)) loggedIn.value = true } catch {} }
  try { const info = await api('/api/version'); appVersion.value = info.version } catch {}
})
</script>

<style>
.app { min-height: 100vh; display: flex; flex-direction: column; }
.app-header {
  position: sticky; top: 0; z-index: 100;
  display: flex; align-items: center; justify-content: space-between;
  padding: 0 24px; height: 56px;
  background: var(--surface-solid);
  border-bottom: 1px solid var(--border);
}
.header-left { display: flex; align-items: center; gap: 10px; }
.header-left h1 { font-size: 15px; font-weight: 600; letter-spacing: -0.2px; }
.header-right { display: flex; align-items: center; gap: 4px; }
.icon-btn {
  display: flex; align-items: center; justify-content: center;
  width: 32px; height: 32px; border: none; border-radius: var(--radius-xs);
  background: transparent; color: var(--text-2); cursor: pointer;
  transition: var(--transition-fast);
}
.icon-btn:hover { background: var(--primary-subtle); color: var(--primary); }
.badge {
  font-size: 11px; font-weight: 500; padding: 2px 7px;
  border-radius: var(--radius-full); background: var(--primary-subtle); color: var(--primary);
}
.menu-btn {
  display: none; align-items: center; justify-content: center;
  width: 32px; height: 32px; padding: 0; background: none; border: none; cursor: pointer;
  color: var(--text-2); border-radius: var(--radius-xs);
}
.menu-btn:hover { background: var(--primary-subtle); color: var(--primary); }
.app-body { display: flex; flex: 1; }
.sidebar {
  width: 210px; min-height: calc(100vh - 56px);
  padding: 12px 8px; overflow-y: auto;
  background: var(--surface-solid);
  border-right: 1px solid var(--border);
  flex-shrink: 0;
}
.nav-group { margin-bottom: 16px; }
.nav-group-label {
  font-size: 10px; font-weight: 600; text-transform: uppercase;
  letter-spacing: 0.8px; color: var(--text-3); padding: 0 10px; margin-bottom: 4px;
}
.nav-group a {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 10px; border-radius: var(--radius-xs);
  color: var(--text-2); text-decoration: none; cursor: pointer;
  font-size: 13px; font-weight: 500; transition: var(--transition-fast);
}
.nav-group a:hover { background: var(--primary-subtle); color: var(--text); }
.nav-group a.active { background: var(--primary); color: white; }
.nav-group a.active .nav-icon { color: white; }
.nav-icon { display: flex; align-items: center; flex-shrink: 0; color: var(--text-3); }
.main-content { flex: 1; padding: 24px 28px; overflow-y: auto; min-height: calc(100vh - 56px); }
.page-header { margin-bottom: 24px; animation: fadeIn 0.25s ease; }
.page-title-row { display: flex; align-items: center; gap: 8px; }
.page-title-row h2 { font-size: 20px; font-weight: 700; letter-spacing: -0.3px; }
.page-icon { display: flex; align-items: center; color: var(--primary); }
.page-desc { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.page-content { animation: slideUp 0.3s ease; }

/* Login */
.login-page {
  min-height: 100vh; display: flex; align-items: center; justify-content: center;
  background: var(--bg);
}
.login-card {
  width: 360px; padding: 36px; text-align: center;
  background: var(--surface-solid);
  border-radius: var(--radius); border: 1px solid var(--border);
  box-shadow: var(--glass-shadow-lg);
}
.login-icon { margin-bottom: 14px; }
.login-title { font-size: 22px; font-weight: 700; letter-spacing: -0.3px; }
.login-desc { font-size: 13px; color: var(--text-2); margin: 4px 0 20px; }
.login-input-group { display: flex; flex-direction: column; gap: 10px; }
.login-input-group input {
  padding: 10px 14px; border-radius: var(--radius-sm);
  border: 1px solid var(--border); background: var(--bg);
  color: var(--text); font-size: 14px; outline: none; transition: var(--transition-fast);
}
.login-input-group input:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-glow); }
.login-btn {
  display: flex; align-items: center; justify-content: center; gap: 8px;
  padding: 10px; border: none; border-radius: var(--radius-sm);
  background: var(--primary);
  color: white; font-size: 14px; font-weight: 600; cursor: pointer;
  transition: var(--transition-fast);
}
.login-btn:hover { background: var(--primary-hover); }
.login-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.login-err { color: var(--danger); font-size: 13px; margin-top: 10px; }
.login-footer { margin-top: 20px; font-size: 12px; color: var(--text-3); display: flex; justify-content: center; gap: 6px; }
.sep { opacity: 0.4; }

/* Overlay for mobile sidebar */
.overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.3); z-index: 89; }

@media (max-width: 768px) {
  .menu-btn { display: flex; }
  .sidebar { position: fixed; left: -240px; top: 56px; bottom: 0; z-index: 90; transition: left 0.25s ease; }
  .sidebar.open { left: 0; }
  .main-content { padding: 16px; }
}
</style>
