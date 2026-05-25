<template>
  <div>
    <div class="section-grid">
      <div class="glass-card">
        <div class="card-header"><h3>担忧 <span class="badge">{{ concerns.length }}</span></h3></div>
        <div v-if="!concerns.length" class="empty">暂无担忧</div>
        <div v-else class="card-list">
          <div v-for="(c, i) in concerns" :key="i" class="list-item">
            <span class="cat-tag" :style="{ background: concernColor(c.category) }">{{ c.category }}</span>
            <span class="list-text">{{ c.content }}</span>
            <span class="list-time">{{ fmtTime(c.created) }}</span>
          </div>
        </div>
      </div>
      <div class="glass-card">
        <div class="card-header"><h3>考量 <span class="badge">{{ deliberations.length }}</span></h3></div>
        <div v-if="!deliberations.length" class="empty">暂无考量</div>
        <div v-else class="card-list">
          <div v-for="(d, i) in deliberations" :key="i" class="list-item">
            <span class="list-text">{{ d.content }}</span>
            <span class="list-time">{{ fmtTime(d.created) }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const concerns = ref([])
const deliberations = ref([])

function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
function concernColor(c) { const m = { social: '#6366f1', task: '#f59e0b', emotional: '#f472b6', self: '#34d399' }; return m[c?.toLowerCase()] || '#6b7280' }

async function load() {
  try { const d = await api('/api/mental-state'); concerns.value = (d.concerns || []).reverse(); deliberations.value = (d.deliberations || []).reverse() } catch {}
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.section-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; display: flex; align-items: center; gap: 8px; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.empty { text-align: center; padding: 32px; color: var(--text-3); font-size: 13px; }
.card-list { display: flex; flex-direction: column; gap: 4px; }
.list-item { display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: var(--radius-xs); transition: var(--transition); flex-wrap: wrap; }
.list-item:hover { background: var(--surface-hover); }
.list-text { flex: 1; font-size: 13px; min-width: 0; }
.list-time { font-size: 11px; color: var(--text-3); white-space: nowrap; }
.cat-tag { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 4px; color: #fff; }
</style>