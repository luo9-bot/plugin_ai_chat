<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="s in stats" :key="s.label">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value ?? '-' }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>
    <div class="card">
      <div class="card-header"><h3>情绪状态</h3></div>
      <div v-if="!emotions.length" class="empty">暂无情绪数据</div>
      <div v-else class="table-wrap">
        <table>
          <thead><tr><th>用户ID</th><th>情绪</th><th>强度</th><th>次要情绪</th><th>互动频率</th></tr></thead>
          <tbody>
            <tr v-for="e in emotions" :key="e.user_id">
              <td class="mono">{{ e.user_id }}</td>
              <td><span class="emotion-badge" :style="{ background: emoColor(e.current) }">{{ emoLabel(e.current) }}</span></td>
              <td><div class="bar-wrap"><div class="bar" :style="{ width: (e.intensity || 0) * 100 + '%', background: emoColor(e.current) }"></div></div><span class="bar-val">{{ ((e.intensity || 0) * 100).toFixed(0) }}%</span></td>
              <td><span v-if="e.secondary" class="emotion-badge" :style="{ background: emoColor(e.secondary), opacity: 0.7 }">{{ emoLabel(e.secondary) }}</span><span v-else class="text-muted">—</span></td>
              <td class="mono">{{ (e.interaction_rate || 0).toFixed(1) }}/h</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const rawData = ref({})
const emotions = computed(() => {
  return Object.entries(rawData.value)
    .filter(([uid]) => uid !== '0')
    .map(([uid, state]) => ({
      user_id: uid,
      current: state?.current || 'neutral',
      secondary: state?.secondary || null,
      intensity: state?.intensity || 0,
      interaction_rate: state?.interaction_rate || 0,
      last_update: state?.last_update || 0,
    }))
    .sort((a, b) => b.intensity - a.intensity)
})
const stats = computed(() => [
  { label: '追踪用户', value: emotions.value.length, sub: '有情绪记录', color: '#6366f1' },
  { label: '活跃情绪', value: [...new Set(emotions.value.map(e => e.current))].length, sub: '不同情绪类型', color: '#34d399' },
])

function emoColor(e) {
  const m = { neutral: '#6b7280', happy: '#34d399', sad: '#60a5fa', thinking: '#8b5cf6', surprised: '#fbbf24', angry: '#ef4444', shy: '#f472b6', worried: '#f97316', tired: '#9ca3af', excited: '#f59e0b', like: '#ec4899' }
  return m[e?.toLowerCase()] || '#6b7280'
}
function emoLabel(e) {
  const m = { neutral: '平静', happy: '开心', sad: '难过', thinking: '沉思', surprised: '惊讶', angry: '不悦', shy: '害羞', worried: '担忧', tired: '疲惫', excited: '兴奋', like: '心动' }
  return m[e?.toLowerCase()] || e || '未知'
}

async function load() {
  try { rawData.value = await api('/api/emotion') } catch {}
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.stat-value { font-size: 28px; font-weight: 700; letter-spacing: -0.5px; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--border); }
td { padding: 8px 12px; border-bottom: 1px solid var(--border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.emotion-badge { padding: 2px 8px; border-radius: 4px; font-size: 11px; font-weight: 500; color: #fff; }
.bar-wrap { width: 80px; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; display: inline-block; vertical-align: middle; }
.bar { height: 100%; border-radius: 3px; transition: width 0.5s ease; }
.bar-val { font-size: 11px; color: var(--text-2); margin-left: 6px; }
.text-muted { color: var(--text-3); }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
</style>
