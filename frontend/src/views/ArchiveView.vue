<template>
  <div>
    <h3>归档数据</h3>
    <div class="stat-grid">
      <div class="stat-card"><div class="label">归档工作记忆</div><div class="value">{{ wm.length }}</div></div>
      <div class="stat-card"><div class="label">归档长期记忆</div><div class="value">{{ lt.length }}</div></div>
    </div>
    <h3>💬 归档工作记忆 (最近100条)</h3>
    <div v-if="!wm.length" class="empty">暂无</div>
    <table v-else><thead><tr><th>群</th><th>用户</th><th>内容</th><th>时间</th><th>归档时间</th></tr></thead><tbody>
      <tr v-for="(e, i) in wm.slice(-100).reverse()" :key="i">
        <td class="mono">{{ e.group_id }}</td><td class="mono">{{ e.user_id }}</td>
        <td class="truncate">{{ e.content }}</td>
        <td class="mono">{{ fmtTime(e.timestamp) }}</td><td class="mono">{{ fmtTime(e.archived_at) }}</td>
      </tr>
    </tbody></table>
    <h3>📦 归档长期记忆 (最近100条)</h3>
    <div v-if="!lt.length" class="empty">暂无</div>
    <table v-else><thead><tr><th>用户</th><th>内容</th><th>重要性</th><th>时间</th><th>归档时间</th></tr></thead><tbody>
      <tr v-for="(e, i) in lt.slice(-100).reverse()" :key="i">
        <td class="mono">{{ e.user_id }}</td><td class="truncate">{{ e.content }}</td>
        <td><span :class="'badge badge-' + (e.importance||'normal').toLowerCase()">{{ e.importance }}</span></td>
        <td class="mono">{{ fmtTime(e.created) }}</td><td class="mono">{{ fmtTime(e.archived_at) }}</td>
      </tr>
    </tbody></table>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const wm = ref([]); const lt = ref([])
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
async function load() { const d = await api('/api/archive'); wm.value = d.working_memory || []; lt.value = d.long_term || [] }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 20px 0 8px; color: var(--text-dim); }
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 12px; margin-bottom: 20px; }
.stat-card { background: var(--surface); border: 2px solid var(--accent-light); border-radius: var(--radius); padding: 16px; text-align: center; box-shadow: var(--shadow); }
.stat-card .label { font-size: 11px; color: var(--text-dim); margin-bottom: 4px; }
.stat-card .value { font-size: 24px; font-weight: 700; background: linear-gradient(135deg, var(--accent), var(--purple)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); margin-bottom: 16px; }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
tr:hover { background: var(--surface2); }
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.badge { display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; font-weight: 500; }
.badge-permanent { background: #fee2e2; color: #ef4444; }
.badge-important { background: #fef3c7; color: #f59e0b; }
.badge-normal { background: #f3e8ff; color: #a855f7; }
.empty { text-align: center; padding: 20px; color: var(--text-dim); }
</style>
