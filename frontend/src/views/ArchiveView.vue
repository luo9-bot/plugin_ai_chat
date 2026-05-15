<template>
  <div>
    <div class="stat-row">
      <div class="stat-card">
        <div class="stat-num">{{ wm.length }}</div>
        <div class="stat-label">归档工作记忆</div>
      </div>
      <div class="stat-card">
        <div class="stat-num">{{ lt.length }}</div>
        <div class="stat-label">归档长期记忆</div>
      </div>
    </div>

    <div class="section">
      <h3>归档工作记忆</h3>
      <div v-if="!wm.length" class="empty">暂无</div>
      <table v-else>
        <thead><tr><th>群</th><th>用户</th><th>内容</th><th>时间</th><th>归档时间</th></tr></thead>
        <tbody>
          <tr v-for="(e, i) in wm.slice(-100).reverse()" :key="i">
            <td class="mono">{{ e.group_id }}</td><td class="mono">{{ e.user_id }}</td>
            <td class="truncate">{{ e.content }}</td>
            <td class="mono">{{ fmtTime(e.timestamp) }}</td><td class="mono">{{ fmtTime(e.archived_at) }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="section">
      <h3>归档长期记忆</h3>
      <div v-if="!lt.length" class="empty">暂无</div>
      <table v-else>
        <thead><tr><th>用户</th><th>内容</th><th>重要性</th><th>时间</th><th>归档时间</th></tr></thead>
        <tbody>
          <tr v-for="(e, i) in lt.slice(-100).reverse()" :key="i">
            <td class="mono">{{ e.user_id }}</td><td class="truncate">{{ e.content }}</td>
            <td><span class="badge" :class="'badge-' + (e.importance||'normal')">{{ e.importance }}</span></td>
            <td class="mono">{{ fmtTime(e.created) }}</td><td class="mono">{{ fmtTime(e.archived_at) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
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
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.badge-permanent { background: var(--danger-bg); color: var(--danger); }
.badge-important { background: var(--warning-bg); color: var(--warning); }
.badge-normal { background: var(--primary-bg); color: var(--primary); }
</style>
