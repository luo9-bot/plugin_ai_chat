<template>
  <div>
    <div class="section-grid">
      <div class="glass-card">
        <div class="card-header"><h3>归档</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
        <div v-if="!archive" class="empty">加载中...</div>
        <div v-else>
          <div class="stat-row">
            <div class="glass-inner stat-item"><span class="stat-num">{{ archive.working_memory || 0 }}</span><span class="stat-lbl">工作记忆</span></div>
            <div class="glass-inner stat-item"><span class="stat-num">{{ archive.long_term || 0 }}</span><span class="stat-lbl">长期归档</span></div>
          </div>
        </div>
      </div>
      <div class="glass-card">
        <div class="card-header"><h3>备份</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
        <div v-if="!backups" class="empty">加载中...</div>
        <div v-else class="card-list">
          <div v-for="(b, i) in backups" :key="i" class="list-item">
            <span class="mono">{{ b.name }}</span>
            <span class="list-time">{{ fmtSize(b.size) }}</span>
            <div class="list-actions">
              <button class="btn btn-ghost btn-xs" @click="restore(b.name)">恢复</button>
              <button class="btn btn-ghost btn-xs" style="color:var(--danger)" @click="del(b.name)">删除</button>
            </div>
          </div>
          <div v-if="!backups.length" class="empty" style="padding:16px">暂无备份</div>
        </div>
        <button class="btn btn-primary" style="margin-top:12px;width:100%" @click="create">＋ 创建备份</button>
      </div>
      <div class="glass-card">
        <div class="card-header"><h3>远程同步</h3></div>
        <div class="config-grid">
          <div class="config-item"><label>状态</label><span>{{ syncStatus || '未知' }}</span></div>
          <div class="config-item"><label>最后同步</label><span>{{ syncTime || '-' }}</span></div>
        </div>
        <div style="display:flex;gap:8px;margin-top:12px">
          <button class="btn btn-primary" style="flex:1" @click="syncAll">同步全部</button>
          <button class="btn btn-ghost" style="flex:1" @click="load">刷新</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const archive = ref(null)
const backups = ref([])
const syncStatus = ref('')
const syncTime = ref('')

function fmtSize(s) { if (!s) return '-'; if (s < 1024) return s + 'B'; if (s < 1024*1024) return (s/1024).toFixed(1) + 'KB'; return (s/1024/1024).toFixed(1) + 'MB' }

async function load() {
  try { archive.value = await api('/api/archive'); backups.value = await api('/api/backups'); const s = await api('/api/sync'); syncStatus.value = s.connected ? '🟢 已连接' : '🔴 未连接'; syncTime.value = s.last_sync || '-' } catch {}
}
async function create() { await api('/api/backups/create', { method: 'POST' }); load() }
async function restore(name) { if (confirm('确认恢复 ' + name + ' ？')) { await api('/api/backups/' + encodeURIComponent(name) + '/restore', { method: 'POST' }); load() } }
async function del(name) { if (confirm('确认删除 ' + name + ' ？')) { await api('/api/backups/' + encodeURIComponent(name) + '/delete', { method: 'POST' }); load() } }
async function syncAll() { await api('/api/sync/sync', { method: 'POST' }); load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.section-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.stat-row { display: flex; gap: 16px; }
.stat-item { flex: 1; text-align: center; padding: 16px; border-radius: var(--radius-sm); background: var(--surface-hover); }
.stat-num { font-size: 28px; font-weight: 700; color: var(--primary); }
.stat-lbl { display: block; font-size: 12px; color: var(--text-2); margin-top: 4px; }
.card-list { display: flex; flex-direction: column; gap: 2px; }
.list-item { display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: var(--radius-xs); }
.list-item:hover { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 13px; flex: 1; }
.list-time { font-size: 11px; color: var(--text-3); }
.list-actions { display: flex; gap: 4px; }
.config-grid { display: flex; flex-direction: column; gap: 8px; }
.config-item { display: flex; justify-content: space-between; padding: 6px 0; font-size: 13px; border-bottom: 1px solid var(--glass-border); }
.config-item label { color: var(--text-2); }
</style>