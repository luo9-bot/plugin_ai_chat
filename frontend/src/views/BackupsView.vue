<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="(count, type) in counts" :key="type">
        <div class="stat-value">{{ count }}</div>
        <div class="stat-label">{{ typeLabel(type) }}</div>
      </div>
    </div>

    <div class="card">
      <div class="card-header">
        <h3>备份详情</h3>
        <div class="header-actions">
          <select v-model="selectedType" class="glass-select">
            <option v-for="t in types" :key="t" :value="t">{{ typeLabel(t) }}</option>
          </select>
          <button class="btn btn-primary btn-sm" @click="createBackup">＋ 创建</button>
          <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
        </div>
      </div>
      <div v-if="loading" class="empty">加载中...</div>
      <div v-else-if="!items.length" class="empty">暂无该类型备份</div>
      <div v-else class="card-list">
        <div v-for="(b, i) in items" :key="i" class="list-item">
          <span class="mono">{{ b.filename }}</span>
          <span class="list-time">{{ fmtSize(b.size) }}</span>
          <div class="list-actions">
            <button class="btn btn-ghost btn-xs" @click="restore(b.filename)">恢复</button>
            <button class="btn btn-ghost btn-xs" style="color:var(--danger)" @click="del(b.filename)">删除</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, watch, onMounted } from 'vue'
import { api } from '../api.js'

const types = ref([])
const counts = ref({})
const selectedType = ref('self_memory')
const items = ref([])
const loading = ref(false)

function typeLabel(t) {
  const m = { self_memory: '自我记忆', memory: '用户记忆', working_memory: '工作记忆', personality: '人格', emotion: '情绪', mental_state: '心理状态', blocklist: '黑名单', proactive: '主动消息', proactive_config: '主动配置', archive: '归档' }
  return m[t] || t
}
function fmtSize(s) { if (!s) return '-'; if (s < 1024) return s + 'B'; if (s < 1024*1024) return (s/1024).toFixed(1) + 'KB'; return (s/1024/1024).toFixed(1) + 'MB' }

async function load() {
  try {
    const d = await api('/api/backups')
    types.value = d.types || []
    counts.value = d.counts || {}
    if (!selectedType.value && types.value.length) selectedType.value = types.value[0]
    await loadItems()
  } catch { loading.value = false }
}

async function loadItems() {
  if (!selectedType.value) return
  loading.value = true
  try {
    const d = await api('/api/backups/' + selectedType.value)
    items.value = d.backups || []
  } catch {}
  loading.value = false
}

watch(selectedType, loadItems)

async function createBackup() {
  await api('/api/backups', { method: 'POST', body: JSON.stringify({ action: 'create', type: selectedType.value }) })
  load()
}
async function restore(filename) {
  if (confirm('确认恢复 ' + filename + ' ？')) {
    await api('/api/backups', { method: 'POST', body: JSON.stringify({ action: 'restore', type: selectedType.value, filename }) })
    load()
  }
}
async function del(filename) {
  if (confirm('确认删除 ' + filename + ' ？')) {
    await api('/api/backups', { method: 'POST', body: JSON.stringify({ action: 'delete', type: selectedType.value, filename }) })
    load()
  }
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 12px; margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 12px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 8px; align-items: center; }
.glass-select { padding: 6px 10px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 12px; outline: none; }
.stat-value { font-size: 24px; font-weight: 700; color: var(--primary); }
.stat-label { font-size: 12px; color: var(--text-2); margin-top: 2px; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.card-list { display: flex; flex-direction: column; gap: 2px; }
.list-item { display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: var(--radius-xs); }
.list-item:hover { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; flex: 1; }
.list-time { font-size: 11px; color: var(--text-3); }
.list-actions { display: flex; gap: 4px; }
</style>