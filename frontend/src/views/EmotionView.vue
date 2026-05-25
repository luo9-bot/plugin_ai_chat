<template>
  <div>
    <div class="stat-grid">
      <div class="glass-card" v-for="(s, i) in stats" :key="i">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value ?? '-' }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>
    <div class="glass-card">
      <div class="card-header"><h3>情绪分析 - 用户消息</h3></div>
      <div v-if="!emotions.length" class="empty">暂无情绪数据</div>
      <div v-else class="table-wrap">
        <table>
          <thead><tr><th>用户</th><th>情绪</th><th>强度</th><th>时间</th></tr></thead>
          <tbody>
            <tr v-for="(e, i) in emotions" :key="i">
              <td class="mono">{{ e.user_id }}</td>
              <td><span class="emotion-badge" :style="{ background: emoColor(e.emotion), color: '#fff' }">{{ e.emotion }}</span></td>
              <td><div class="bar-wrap"><div class="bar" :style="{ width: (e.intensity || 0) * 100 + '%', background: emoColor(e.emotion) }"></div></div></td>
              <td class="mono">{{ fmtTime(e.timestamp) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const emotions = ref([])
const stats = ref([])

function emoColor(e) { const m = { neutral: '#6b7280', happy: '#34d399', sad: '#60a5fa', thinking: '#8b5cf6', surprised: '#fbbf24', angry: '#ef4444', shy: '#f472b6', worried: '#f97316', tired: '#6b7280', excited: '#f59e0b' }; return m[e?.toLowerCase()] || '#6b7280' }
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

async function load() {
  try { const d = await api('/api/emotion'); emotions.value = (d.history || []).reverse().slice(0, 100); stats.value = [{ label: '追踪中', value: d.user_count, sub: '用户', color: '#6366f1' }, { label: '当前状态', value: d.current_state || 'neutral', sub: 'bot', color: '#34d399' }] } catch {}
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.stat-value { font-size: 28px; font-weight: 700; letter-spacing: -0.5px; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--glass-border); }
td { padding: 8px 12px; border-bottom: 1px solid var(--glass-border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.emotion-badge { padding: 2px 8px; border-radius: 4px; font-size: 11px; font-weight: 500; }
.bar-wrap { width: 80px; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; }
.bar { height: 100%; border-radius: 3px; transition: width 0.5s ease; }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
</style>