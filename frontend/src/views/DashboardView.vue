<template>
  <div>
    <div class="stat-row">
      <div class="stat-card" v-for="s in cards" :key="s.label">
        <div class="stat-num">{{ s.value ?? '-' }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>

    <div class="grid">
      <div class="section">
        <h3>
          <svg viewBox="0 0 18 18" fill="none" width="16" height="16"><path d="M5 3h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2z" stroke="currentColor" stroke-width="1.5"/><path d="M7 7h4M7 10h5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          数据存储
        </h3>
        <div class="metric-list">
          <div class="metric-item"><span>记忆条目</span><span class="val">{{ stats.memory_entries ?? '-' }}</span></div>
          <div class="metric-item"><span>记忆用户</span><span class="val">{{ stats.memory_users ?? 0 }}</span></div>
          <div class="metric-item"><span>表情包</span><span class="val">{{ stats.sticker_count ?? '-' }}</span></div>
          <div class="metric-item"><span>情绪追踪用户</span><span class="val">{{ stats.emotion_users ?? '-' }}</span></div>
        </div>
      </div>

      <div class="section">
        <h3>
          <svg viewBox="0 0 18 18" fill="none" width="16" height="16"><circle cx="9" cy="9" r="7" stroke="currentColor" stroke-width="1.5"/><path d="M9 5v4l3 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          活跃会话
        </h3>
        <div class="metric-list">
          <div class="metric-item"><span>活跃群聊</span><span class="val">{{ stats.active_groups ?? '-' }}</span></div>
          <div class="metric-item"><span>活跃私聊</span><span class="val">{{ stats.active_users ?? '-' }}</span></div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const stats = ref({})

const cards = ref([
  { label: '记忆条目', value: '-', sub: '长期记忆', key: 'memory_entries' },
  { label: '记忆用户', value: '-', sub: '已记录用户', key: 'memory_users' },
  { label: '表情包', value: '-', sub: '已注册', key: 'sticker_count' },
  { label: '活跃群聊', value: '-', sub: '进行中', key: 'active_groups' },
  { label: '活跃私聊', value: '-', sub: '进行中', key: 'active_users' },
  { label: '情绪用户', value: '-', sub: '追踪中', key: 'emotion_users' },
])

let interval = null

async function load() {
  try {
    const d = await api('/api/dashboard')
    stats.value = d
    for (const c of cards.value) {
      if (d[c.key] !== undefined) c.value = d[c.key]
    }
  } catch {}
}

onMounted(() => { load(); interval = setInterval(load, 15000); window.addEventListener('refresh-all', load) })
onUnmounted(() => { clearInterval(interval); window.removeEventListener('refresh-all', load) })
</script>

<style scoped>
.metric-list { display: flex; flex-direction: column; gap: 2px; }
.metric-item {
  display: flex; justify-content: space-between; align-items: center;
  padding: 8px 12px; border-radius: 6px;
}
.metric-item:hover { background: var(--surface-hover); }
.metric-item span:first-child { font-size: 13px; color: var(--text-2); }
.metric-item .val { font-weight: 600; font-size: 14px; color: var(--text); }
</style>
