<template>
  <div>
    <div class="glass-card">
      <div class="card-header"><h3>远程同步</h3></div>
      <div v-if="!status" class="empty">加载中...</div>
      <div v-else>
        <div class="config-grid">
          <div class="config-item"><label>启用</label><span :style="{ color: status.enabled ? 'var(--success)' : 'var(--text-3)' }">{{ status.enabled ? '是' : '否' }}</span></div>
          <div class="config-item"><label>API 地址</label><span class="mono">{{ status.api_url || '—' }}</span></div>
          <div class="config-item"><label>数据库</label><span>{{ status.db_name || '—' }}</span></div>
        </div>
        <div v-if="status.enabled" style="display:flex;gap:8px;margin-top:12px">
          <button class="btn btn-primary" style="flex:1" @click="pushNow">推送全部</button>
          <button class="btn btn-ghost" style="flex:1" @click="load">刷新</button>
        </div>
        <div v-else style="margin-top:12px;padding:12px;background:var(--surface-hover);border-radius:var(--radius-sm);font-size:12px;color:var(--text-2)">
          同步未启用，请在配置中设置 sync.enabled 和相关参数
        </div>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const status = ref(null)
async function load() { try { status.value = await api('/api/sync/status') } catch {} }
async function pushNow() {
  try {
    const r = await api('/api/sync/push', { method: 'POST', body: JSON.stringify({ type: 'self_memory' }) })
    alert('已推送，同步了 ' + (r.synced || 0) + ' 条')
  } catch (e) { alert('推送失败: ' + e.message) }
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>
<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.config-grid { display: flex; flex-direction: column; gap: 8px; }
.config-item { display: flex; justify-content: space-between; padding: 8px 0; font-size: 13px; border-bottom: 1px solid var(--glass-border); }
.config-item label { color: var(--text-2); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
</style>
