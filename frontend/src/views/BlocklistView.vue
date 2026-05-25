<template>
  <div>
    <div class="section-grid">
      <div class="glass-card">
        <div class="card-header"><h3>黑名单</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
        <div v-if="!list.length" class="empty">暂无黑名单用户</div>
        <div v-else class="card-list">
          <div v-for="(u, i) in list" :key="i" class="list-item">
            <span class="mono">{{ u }}</span>
            <button class="btn btn-ghost btn-xs" @click="remove(u)">移除</button>
          </div>
        </div>
      </div>
      <div class="glass-card">
        <div class="card-header"><h3>防注入</h3></div>
        <div class="config-grid">
          <div class="config-item"><label>输入长度限制</label><span>{{ config?.input?.max_message_length || 2000 }}</span></div>
          <div class="config-item"><label>敏感操作</label><span>{{ config?.input?.sensitive_action || 'replace' }}</span></div>
          <div class="config-item"><label>速率限制</label><span>{{ config?.behavior?.rate_limit ? '🟢 开启' : '🔴 关闭' }}</span></div>
          <div class="config-item"><label>自动封禁</label><span>{{ config?.behavior?.auto_ban ? '🟢 开启' : '🔴 关闭' }}</span></div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const list = ref([])
const config = ref(null)

async function load() {
  try {
    const d = await api('/api/blocklist')
    list.value = d.list || []
    config.value = await api('/api/anti-injection')
  } catch {}
}
async function remove(id) { await api('/api/blocklist/' + id + '/remove', { method: 'POST' }); load() }
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.section-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.card-list { display: flex; flex-direction: column; gap: 2px; }
.list-item { display: flex; align-items: center; justify-content: space-between; padding: 8px; border-radius: var(--radius-xs); }
.list-item:hover { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 13px; }
.config-grid { display: flex; flex-direction: column; gap: 8px; }
.config-item { display: flex; justify-content: space-between; padding: 6px 0; font-size: 13px; border-bottom: 1px solid var(--glass-border); }
.config-item label { color: var(--text-2); font-weight: 500; }
</style>