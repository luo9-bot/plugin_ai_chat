<template>
  <div>
    <div class="glass-card">
      <div class="card-header"><h3>备份</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
      <div v-if="!list" class="empty">加载中...</div>
      <div v-else>
        <div v-for="(b, i) in list" :key="i" class="backup-item">
          <span class="mono">{{ b.name }}</span>
          <span class="list-time">{{ fmtSize(b.size) }}</span>
          <div class="backup-actions">
            <button class="btn btn-ghost btn-xs" @click="restore(b.name)">恢复</button>
            <button class="btn btn-ghost btn-xs" style="color:var(--danger)" @click="del(b.name)">删除</button>
          </div>
        </div>
        <div v-if="!list.length" class="empty" style="padding:16px">暂无备份</div>
        <button class="btn btn-primary" style="margin-top:12px;width:100%" @click="create">＋ 创建备份</button>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const list = ref([])
function fmtSize(s) { if (!s) return '-'; if (s < 1024) return s + 'B'; if (s < 1024*1024) return (s/1024).toFixed(1) + 'KB'; return (s/1024/1024).toFixed(1) + 'MB' }
async function load() { try { list.value = await api('/api/backups') } catch {} }
async function restore(n) { if (confirm('确认恢复 ' + n + ' ？')) { await api('/api/backups/' + encodeURIComponent(n) + '/restore', { method: 'POST' }); load() } }
async function del(n) { if (confirm('确认删除 ' + n + ' ？')) { await api('/api/backups/' + encodeURIComponent(n) + '/delete', { method: 'POST' }); load() } }
async function create() { await api('/api/backups/create', { method: 'POST' }); load() }
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>
<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.backup-item { display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: var(--radius-xs); }
.backup-item:hover { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 13px; flex: 1; }
.list-time { font-size: 11px; color: var(--text-3); }
.backup-actions { display: flex; gap: 4px; }
</style>