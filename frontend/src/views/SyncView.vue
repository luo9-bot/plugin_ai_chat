<template>
  <div>
    <div class="glass-card">
      <div class="card-header"><h3>远程同步</h3></div>
      <div v-if="!status" class="empty">加载中...</div>
      <div v-else>
        <div class="config-grid">
          <div class="config-item"><label>连接状态</label><span>{{ status.connected ? '🟢 已连接' : '🔴 未连接' }}</span></div>
          <div class="config-item"><label>最后同步</label><span>{{ status.last_sync ? fmtTime(status.last_sync) : '-' }}</span></div>
          <div class="config-item"><label>同步计数</label><span>{{ status.sync_count || 0 }}</span></div>
        </div>
        <div style="display:flex;gap:8px;margin-top:12px">
          <button class="btn btn-primary" style="flex:1" @click="syncNow">同步全部</button>
          <button class="btn btn-ghost" style="flex:1" @click="load">刷新</button>
        </div>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const status = ref(null)
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
async function load() { try { status.value = await api('/api/sync') } catch {} }
async function syncNow() { await api('/api/sync/sync', { method: 'POST' }); load() }
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>
<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.config-grid { display: flex; flex-direction: column; gap: 8px; }
.config-item { display: flex; justify-content: space-between; padding: 8px 0; font-size: 13px; border-bottom: 1px solid var(--glass-border); }
.config-item label { color: var(--text-2); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
</style>