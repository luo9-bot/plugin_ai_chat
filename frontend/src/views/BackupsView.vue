<template>
  <div>
    <div class="glass-card">
      <div class="card-header"><h3>备份管理</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
      <div v-if="!backups" class="empty">加载中...</div>
      <div v-else>
        <div v-for="(items, type) in backups" :key="type" style="margin-bottom:16px">
          <div class="section-label">{{ type }}</div>
          <div v-if="!items.length" class="empty" style="padding:8px">暂无备份</div>
          <div v-else class="card-list">
            <div v-for="(b, i) in items" :key="i" class="list-item">
              <span class="mono">{{ b.name }}</span>
              <span class="list-time">{{ fmtSize(b.size) }}</span>
              <span class="list-time">{{ fmtTime(b.created_at) }}</span>
              <div class="list-actions">
                <button class="btn btn-ghost btn-xs" @click="restore(type, b.name)">恢复</button>
                <button class="btn btn-ghost btn-xs" style="color:var(--danger)" @click="del(type, b.name)">删除</button>
              </div>
            </div>
          </div>
        </div>
        <div v-if="!Object.keys(backups).length" class="empty" style="padding:16px">暂无备份</div>
        <div style="display:flex;gap:8px;margin-top:12px">
          <button class="btn btn-primary" style="flex:1" @click="create('all')">＋ 创建全部备份</button>
          <button class="btn btn-ghost" style="flex:1" @click="create('self_memory')">自我记忆</button>
          <button class="btn btn-ghost" style="flex:1" @click="create('memory')">用户记忆</button>
          <button class="btn btn-ghost" style="flex:1" @click="create('emotion')">情绪</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const backups = ref({})

function fmtSize(s) { if (!s) return '-'; if (s < 1024) return s + 'B'; if (s < 1024*1024) return (s/1024).toFixed(1) + 'KB'; return (s/1024/1024).toFixed(1) + 'MB' }
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

async function load() { try { backups.value = await api('/api/backups') } catch {} }
async function create(type) { await api('/api/backups/create?type=' + type, { method: 'POST' }); load() }
async function restore(type, name) { if (confirm('确认恢复 ' + name + ' ？')) { await api('/api/backups/' + type + '/restore?name=' + encodeURIComponent(name), { method: 'POST' }); load() } }
async function del(type, name) { if (confirm('确认删除 ' + name + ' ？')) { await api('/api/backups/' + type + '/delete?name=' + encodeURIComponent(name), { method: 'POST' }); load() } }

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
.section-label { font-size: 11px; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 6px; }
.card-list { display: flex; flex-direction: column; gap: 2px; }
.list-item { display: flex; align-items: center; gap: 8px; padding: 8px; border-radius: var(--radius-xs); }
.list-item:hover { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; flex: 1; }
.list-time { font-size: 11px; color: var(--text-3); min-width: 60px; }
.list-actions { display: flex; gap: 4px; }
</style>