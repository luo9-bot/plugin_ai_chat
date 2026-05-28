<template>
  <div>
    <div class="card">
      <div class="card-header">
        <h3>黑名单管理</h3>
        <div class="header-actions">
          <input v-model="addId" placeholder="输入 QQ 号添加" class="glass-input" style="width:160px" />
          <button class="btn btn-primary btn-sm" @click="addUser" :disabled="!addId.trim()">＋ 添加</button>
          <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
        </div>
      </div>
      <div v-if="!list.length" class="empty">暂无黑名单用户</div>
      <div v-else class="chip-list">
        <div v-for="(u, i) in list" :key="i" class="chip">
          <span class="mono">{{ u }}</span>
          <button class="chip-close" @click="remove(u)">✕</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const list = ref([])
const addId = ref('')

async function load() {
  try {
    const d = await api('/api/blocklist')
    list.value = d.blocked || []
  } catch {}
}
async function remove(id) { await api('/api/blocklist/' + id, { method: 'DELETE' }); load() }
async function addUser() {
  if (!addId.value.trim()) return
  await api('/api/blocklist', { method: 'POST', body: JSON.stringify({ user_id: Number(addId.value.trim()) }) })
  addId.value = ''
  load()
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 6px; align-items: center; }
.glass-input { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.chip-list { display: flex; flex-wrap: wrap; gap: 8px; }
.chip { display: flex; align-items: center; gap: 6px; padding: 6px 12px; background: var(--surface-hover); border-radius: 20px; font-size: 13px; }
.chip-close { background: none; border: none; cursor: pointer; color: var(--text-3); font-size: 12px; padding: 0; }
.chip-close:hover { color: var(--danger); }
.mono { font-family: monospace; font-size: 13px; }
</style>