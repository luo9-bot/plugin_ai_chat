<template>
  <div>
    <div class="card">
      <div class="card-header"><h3>归档统计</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
      <div v-if="!stats" class="empty">加载中...</div>
      <div v-else>
        <div class="stat-row">
          <div class="stat-item"><div class="stat-num">{{ Array.isArray(stats.working_memory) ? stats.working_memory.length : stats.working_memory || 0 }}</div><div class="stat-lbl">工作记忆归档</div></div>
          <div class="stat-item"><div class="stat-num">{{ Array.isArray(stats.long_term) ? stats.long_term.length : stats.long_term || 0 }}</div><div class="stat-lbl">长期记忆归档</div></div>
        </div>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const stats = ref(null)
async function load() { try { stats.value = await api('/api/archive') } catch {} }
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>
<style scoped>
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.stat-row { display: flex; gap: 16px; }
.stat-item { flex: 1; text-align: center; padding: 24px; border-radius: var(--radius-sm); background: var(--surface-hover); }
.stat-num { font-size: 32px; font-weight: 700; color: var(--primary); }
.stat-lbl { display: block; font-size: 13px; color: var(--text-2); margin-top: 4px; }
</style>