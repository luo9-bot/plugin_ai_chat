<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="card in statCards" :key="card.label">
        <div class="stat-icon" :style="{ color: card.color }" v-html="card.icon"></div>
        <div class="stat-info">
          <div class="stat-value">{{ card.value ?? '-' }}</div>
          <div class="stat-label">{{ card.label }}</div>
          <div class="stat-sub">{{ card.sub }}</div>
        </div>
        <div class="stat-trend" v-if="card.trend">
          <span :class="card.trend > 0 ? 'up' : 'down'">{{ card.trend > 0 ? '+' : '' }}{{ card.trend }}%</span>
        </div>
      </div>
    </div>

    <div class="chart-grid">
      <div class="card chart-card">
        <h3 class="card-title">记忆分布</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="160" height="160">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--border)" stroke-width="16"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--primary)" stroke-width="16"
              :stroke-dasharray="memArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 0.8s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="26" font-weight="700">{{ stats.memory_entries ?? 0 }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="11">条记忆</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:var(--primary)"></span> 全局记忆</div>
            <div class="legend-item"><span class="dot" style="background:#6366f1"></span> 群记忆</div>
            <div class="legend-item"><span class="dot" style="background:#8b5cf6"></span> 表情包</div>
          </div>
        </div>
      </div>

      <div class="card chart-card">
        <h3 class="card-title">活跃会话</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="160" height="160">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--border)" stroke-width="16"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--info)" stroke-width="16"
              :stroke-dasharray="activeArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 0.8s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="26" font-weight="700">{{ activeTotal }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="11">活跃</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:var(--info)"></span> 群聊: {{ stats.active_groups ?? 0 }}</div>
            <div class="legend-item"><span class="dot" style="background:#60a5fa"></span> 私聊: {{ stats.active_users ?? 0 }}</div>
          </div>
        </div>
      </div>

      <div class="card chart-card">
        <h3 class="card-title">情绪分布</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="160" height="160">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--border)" stroke-width="16"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--warning)" stroke-width="16"
              :stroke-dasharray="emotionArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 0.8s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="26" font-weight="700">{{ stats.emotion_users ?? 0 }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="11">用户</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:var(--warning)"></span> 情绪追踪用户</div>
            <div class="legend-item"><span class="dot" style="background:#fbbf24"></span> 记忆用户</div>
          </div>
        </div>
      </div>
    </div>

    <div class="card data-card">
      <h3 class="card-title">最近活跃</h3>
      <div v-if="activeGroups.length || activeUsers.length" class="active-lists">
        <div v-if="activeGroups.length" class="active-section">
          <div class="active-section-title">群聊 ({{ activeGroups.length }})</div>
          <div class="active-chips">
            <span v-for="g in activeGroups" :key="g" class="chip">群 {{ g }}</span>
          </div>
        </div>
        <div v-if="activeUsers.length" class="active-section">
          <div class="active-section-title">私聊 ({{ activeUsers.length }})</div>
          <div class="active-chips">
            <span v-for="u in activeUsers" :key="u" class="chip">用户 {{ u }}</span>
          </div>
        </div>
      </div>
      <div v-else class="empty-sm">暂无活跃会话</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const stats = ref({})
const activeGroups = ref([])
const activeUsers = ref([])
let interval = null

const I = {
  brain: '<svg viewBox="0 0 24 24" fill="none" width="20" height="20"><path d="M12 3a4 4 0 00-4 4c0 1.5.7 2.8 1.7 3.7C8.3 11.5 7 13 7 15v1a4 4 0 008 0v-1c0-2-1.3-3.5-2.7-4.3C14.3 9.8 15 8.5 15 7a4 4 0 00-3-3.9z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  chat: '<svg viewBox="0 0 24 24" fill="none" width="20" height="20"><path d="M4 12a8 8 0 1116 0H4z" stroke="currentColor" stroke-width="1.5"/><path d="M8 8h8M8 12h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  smile: '<svg viewBox="0 0 24 24" fill="none" width="20" height="20"><circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.5"/><circle cx="9" cy="10" r="1" fill="currentColor"/><circle cx="15" cy="10" r="1" fill="currentColor"/><path d="M8 14c1 1.5 2.5 2 4 2s3-.5 4-2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  db: '<svg viewBox="0 0 24 24" fill="none" width="20" height="20"><ellipse cx="12" cy="6" rx="8" ry="3" stroke="currentColor" stroke-width="1.5"/><path d="M4 6v4c0 1.7 3.6 3 8 3s8-1.3 8-3V6" stroke="currentColor" stroke-width="1.5"/><path d="M4 10v4c0 1.7 3.6 3 8 3s8-1.3 8-3v-4" stroke="currentColor" stroke-width="1.5"/></svg>',
  sticker: '<svg viewBox="0 0 24 24" fill="none" width="20" height="20"><rect x="4" y="4" width="16" height="16" rx="2" stroke="currentColor" stroke-width="1.5"/><circle cx="9" cy="10" r="1.5" fill="currentColor"/><path d="M6 18l4-5 4 5H6z" fill="currentColor" opacity="0.5"/><path d="M13 16l3-5 3 5H13z" fill="currentColor" opacity="0.5"/></svg>',
}

const statCards = computed(() => [
  { label: '记忆条目', value: stats.value.memory_entries, sub: '长期记忆总计', icon: I.brain, color: 'var(--primary)', trend: null },
  { label: '记忆用户', value: stats.value.memory_users, sub: '已记录用户', icon: I.db, color: '#6366f1', trend: null },
  { label: '表情包', value: stats.value.sticker_count, sub: '已注册', icon: I.sticker, color: 'var(--warning)', trend: null },
  { label: '群聊', value: stats.value.active_groups, sub: '进行中', icon: I.chat, color: 'var(--info)', trend: null },
  { label: '私聊', value: stats.value.active_users, sub: '进行中', icon: I.chat, color: '#60a5fa', trend: null },
  { label: '情绪用户', value: stats.value.emotion_users, sub: '追踪中', icon: I.smile, color: 'var(--warning)', trend: null },
])

const activeTotal = computed(() => (stats.value.active_groups || 0) + (stats.value.active_users || 0))
const memArc = computed(() => {
  const total = stats.value.memory_entries || 0
  const pct = Math.min(total / 1000, 1)
  const circ = 2 * Math.PI * 64
  return `${circ * pct} ${circ * (1 - pct)}`
})
const activeArc = computed(() => {
  const total = activeTotal.value
  const pct = Math.min(total / 20, 1)
  const circ = 2 * Math.PI * 64
  return `${circ * pct} ${circ * (1 - pct)}`
})
const emotionArc = computed(() => {
  const total = stats.value.emotion_users || 0
  const pct = Math.min(total / 50, 1)
  const circ = 2 * Math.PI * 64
  return `${circ * pct} ${circ * (1 - pct)}`
})

async function load() {
  try {
    stats.value = await api('/api/dashboard')
    const conv = await api('/api/conversations')
    activeGroups.value = conv.groups || []
    activeUsers.value = conv.private_users || []
  } catch {}
}

onMounted(() => { load(); interval = setInterval(load, 15000); window.addEventListener('refresh-all', load) })
onUnmounted(() => { clearInterval(interval); window.removeEventListener('refresh-all', load) })
</script>

<style scoped>
.card {
  padding: 18px; border-radius: var(--radius);
  background: var(--surface-solid);
  border: 1px solid var(--border);
  box-shadow: var(--glass-shadow);
  transition: var(--transition);
}
.card:hover { box-shadow: var(--glass-shadow-lg); }
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 12px; margin-bottom: 20px; }
.stat-icon { margin-bottom: 10px; }
.stat-value { font-size: 26px; font-weight: 700; letter-spacing: -0.5px; line-height: 1; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; font-weight: 500; }
.stat-sub { font-size: 11px; color: var(--text-3); margin-top: 2px; }
.chart-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px; margin-bottom: 20px; }
.chart-card { text-align: center; }
.card-title { font-size: 13px; font-weight: 600; margin-bottom: 14px; text-align: left; color: var(--text-2); }
.chart-container { display: flex; align-items: center; justify-content: center; gap: 20px; flex-wrap: wrap; }
.chart-legend { display: flex; flex-direction: column; gap: 6px; text-align: left; }
.legend-item { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-2); }
.dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.data-card { margin-bottom: 20px; }
.active-lists { display: flex; flex-direction: column; gap: 14px; }
.active-section-title { font-size: 12px; font-weight: 600; color: var(--text-2); margin-bottom: 6px; }
.active-chips { display: flex; flex-wrap: wrap; gap: 6px; }
.chip {
  font-size: 12px; padding: 4px 10px;
  background: var(--primary-subtle); border-radius: var(--radius-full);
  color: var(--primary); font-family: system-ui, sans-serif; font-weight: 500;
}
.empty-sm { text-align: center; padding: 20px; color: var(--text-3); font-size: 13px; }
@media (max-width: 768px) {
  .stat-grid { grid-template-columns: repeat(2, 1fr); }
  .chart-grid { grid-template-columns: 1fr; }
  .chart-container { flex-direction: column; }
}
@media (max-width: 480px) {
  .stat-grid { grid-template-columns: 1fr; }
}
</style>
