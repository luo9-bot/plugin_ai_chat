<template>
  <div>
    <div class="stat-grid">
      <div class="glass-card" v-for="card in statCards" :key="card.label">
        <div class="stat-icon" :style="{ color: card.color }" v-html="card.icon"></div>
        <div class="stat-info">
          <div class="stat-value">{{ card.value ?? '-' }}</div>
          <div class="stat-label">{{ card.label }}</div>
          <div class="stat-sub">{{ card.sub }}</div>
        </div>
        <div class="stat-trend" v-if="card.trend">
          <span :class="card.trend > 0 ? 'up' : 'down'">{{ card.trend > 0 ? '↑' : '↓' }} {{ Math.abs(card.trend) }}%</span>
        </div>
      </div>
    </div>

    <div class="chart-grid">
      <div class="glass-card chart-card">
        <h3 class="card-title">记忆分布</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="180" height="180">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--surface)" stroke-width="20"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="#6366f1" stroke-width="20"
              :stroke-dasharray="memArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 1s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="28" font-weight="700">{{ stats.memory_entries ?? 0 }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="11">条记忆</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:#6366f1"></span> 全局记忆</div>
            <div class="legend-item"><span class="dot" style="background:#8b5cf6"></span> 群记忆</div>
            <div class="legend-item"><span class="dot" style="background:#a78bfa"></span> 表情包</div>
          </div>
        </div>
      </div>

      <div class="glass-card chart-card">
        <h3 class="card-title">活跃会话</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="180" height="180">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--surface)" stroke-width="20"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="#34d399" stroke-width="20"
              :stroke-dasharray="activeArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 1s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="28" font-weight="700">{{ activeTotal }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="11">活跃</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:#34d399"></span> 群聊: {{ stats.active_groups ?? 0 }}</div>
            <div class="legend-item"><span class="dot" style="background:#6ee7b7"></span> 私聊: {{ stats.active_users ?? 0 }}</div>
          </div>
        </div>
      </div>

      <div class="glass-card chart-card">
        <h3 class="card-title">情绪分布</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="180" height="180">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--surface)" stroke-width="20"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="#fbbf24" stroke-width="20"
              :stroke-dasharray="emotionArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 1s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="28" font-weight="700">{{ stats.emotion_users ?? 0 }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="11">用户</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:#fbbf24"></span> 情绪追踪用户</div>
            <div class="legend-item"><span class="dot" style="background:#f59e0b"></span> 记忆用户</div>
          </div>
        </div>
      </div>
    </div>

    <div class="glass-card data-card">
      <h3 class="card-title">数据存储结构</h3>
      <div class="file-tree">
        <div class="tree-item dir">📁 memory/</div>
        <div class="tree-item dir" style="padding-left:24px">📁 users/</div>
        <div class="tree-item file" style="padding-left:48px">📄 512166443.json  <span class="tree-meta">— 用户全局记忆</span></div>
        <div class="tree-item file" style="padding-left:48px">📄 2950726483.json</div>
        <div class="tree-item dir" style="padding-left:24px">📁 groups/</div>
        <div class="tree-item dir" style="padding-left:48px">📁 676426335/</div>
        <div class="tree-item file" style="padding-left:72px">📄 group.json  <span class="tree-meta">— 群级别记忆</span></div>
        <div class="tree-item file" style="padding-left:72px">📄 512166443.json  <span class="tree-meta">— 群内用户记忆</span></div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'

const stats = ref({})
let interval = null

const I = {
  brain: '<svg viewBox="0 0 24 24" fill="none" width="22" height="22"><path d="M12 3a4 4 0 00-4 4c0 1.5.7 2.8 1.7 3.7C8.3 11.5 7 13 7 15v1a4 4 0 008 0v-1c0-2-1.3-3.5-2.7-4.3C14.3 9.8 15 8.5 15 7a4 4 0 00-3-3.9z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  chat: '<svg viewBox="0 0 24 24" fill="none" width="22" height="22"><path d="M4 12a8 8 0 1116 0H4z" stroke="currentColor" stroke-width="1.5"/><path d="M8 8h8M8 12h6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  smile: '<svg viewBox="0 0 24 24" fill="none" width="22" height="22"><circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.5"/><path d="M8 14c1 1.5 2.5 2 4 2s3-.5 4-2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  db: '<svg viewBox="0 0 24 24" fill="none" width="22" height="22"><ellipse cx="12" cy="6" rx="8" ry="3" stroke="currentColor" stroke-width="1.5"/><path d="M4 6v4c0 1.7 3.6 3 8 3s8-1.3 8-3V6" stroke="currentColor" stroke-width="1.5"/><path d="M4 10v4c0 1.7 3.6 3 8 3s8-1.3 8-3v-4" stroke="currentColor" stroke-width="1.5"/></svg>',
  sticker: '<svg viewBox="0 0 24 24" fill="none" width="22" height="22"><path d="M4 7a3 3 0 013-3h10a3 3 0 013 3v8a3 3 0 01-3 3H7a3 3 0 01-3-3V7z" stroke="currentColor" stroke-width="1.5"/><circle cx="9" cy="10" r="1.5" fill="currentColor"/><path d="M6 17l4-5 4 5H6z" fill="currentColor" opacity="0.5"/><path d="M13 15l3-5 3 5H13z" fill="currentColor" opacity="0.5"/></svg>',
  shield: '<svg viewBox="0 0 24 24" fill="none" width="22" height="22"><path d="M12 3l7 3v5c0 4-3 7-7 8-4-1-7-4-7-8V6l7-3z" stroke="currentColor" stroke-width="1.5"/><path d="M9 12l2 2 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
}

const statCards = computed(() => [
  { label: '记忆条目', value: stats.value.memory_entries, sub: '长期记忆总计', icon: I.brain, color: '#6366f1', trend: null },
  { label: '记忆用户', value: stats.value.memory_users, sub: '已记录用户', icon: I.db, color: '#8b5cf6', trend: null },
  { label: '表情包', value: stats.value.sticker_count, sub: '已注册', icon: I.sticker, color: '#f59e0b', trend: null },
  { label: '群聊', value: stats.value.active_groups, sub: '进行中', icon: I.chat, color: '#34d399', trend: null },
  { label: '私聊', value: stats.value.active_users, sub: '进行中', icon: I.chat, color: '#6ee7b7', trend: null },
  { label: '情绪用户', value: stats.value.emotion_users, sub: '追踪中', icon: I.smile, color: '#fbbf24', trend: null },
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
  try { const d = await (await fetch('/api/dashboard', { headers: { 'Authorization': 'Bearer ' + localStorage.getItem('admin_token') } })).json(); stats.value = d } catch {}
}

onMounted(() => { load(); interval = setInterval(load, 15000); window.addEventListener('refresh-all', load) })
onUnmounted(() => { clearInterval(interval); window.removeEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 16px; margin-bottom: 24px; }
.glass-card {
  padding: 20px; border-radius: var(--radius);
  backdrop-filter: blur(16px) saturate(1.5);
  -webkit-backdrop-filter: blur(16px) saturate(1.5);
  background: var(--surface); border: 1px solid var(--glass-border);
  box-shadow: var(--glass-shadow); transition: var(--transition);
}
.glass-card:hover { transform: translateY(-2px); box-shadow: 0 12px 40px rgba(0,0,0,0.12); }
.stat-card { position: relative; }
.stat-icon { margin-bottom: 12px; }
.stat-value { font-size: 28px; font-weight: 700; letter-spacing: -0.5px; line-height: 1; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; font-weight: 500; }
.stat-sub { font-size: 11px; color: var(--text-3); margin-top: 2px; }
.chart-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; margin-bottom: 24px; }
.chart-card { text-align: center; }
.card-title { font-size: 14px; font-weight: 600; margin-bottom: 16px; text-align: left; }
.chart-container { display: flex; align-items: center; justify-content: center; gap: 20px; flex-wrap: wrap; }
.chart-legend { display: flex; flex-direction: column; gap: 8px; text-align: left; }
.legend-item { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-2); }
.dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.data-card { margin-bottom: 24px; }
.file-tree { font-family: monospace; font-size: 12px; line-height: 1.8; }
.tree-item { padding: 2px 0; }
.tree-item.dir { color: var(--primary); font-weight: 600; }
.tree-item.file { color: var(--text-2); }
.tree-meta { color: var(--text-3); font-size: 11px; }
</style>