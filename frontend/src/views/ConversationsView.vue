<template>
  <div>
    <div class="glass-card">
      <div class="card-header">
        <h3>对话管理</h3>
        <div class="header-actions">
          <input v-model="search" placeholder="搜索群号..." class="glass-input" />
          <button class="btn btn-ghost" @click="load">↻ 刷新</button>
        </div>
      </div>
      <div v-if="!convs" class="empty">加载中...</div>
      <div v-else>
        <div class="section-label">群聊 ({{ convs.groups?.length || 0 }})</div>
        <div class="chip-list">
          <div v-for="g in filteredGroups" :key="g" class="chip">
            <span>{{ g }}</span>
            <button class="chip-close" @click="toggle(g, 'group')">✕</button>
          </div>
          <div v-if="!convs.groups?.length" class="empty" style="padding:16px">无活跃群聊</div>
        </div>
        <div class="section-label" style="margin-top:16px">私聊 ({{ convs.private_users?.length || 0 }})</div>
        <div class="chip-list">
          <div v-for="u in convs.private_users" :key="u" class="chip chip-priv">
            <span>{{ u }}</span>
            <button class="chip-close" @click="toggle(u, 'private')">✕</button>
          </div>
          <div v-if="!convs.private_users?.length" class="empty" style="padding:16px">无活跃私聊</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const convs = ref(null)
const search = ref('')

const filteredGroups = computed(() => {
  if (!search.value) return convs.value?.groups || []
  return (convs.value?.groups || []).filter(g => String(g).includes(search.value))
})

async function load() { try { convs.value = await api('/api/conversations') } catch {} }
async function toggle(id, kind) { await api(`/api/conversations/${kind}/${id}/disable`, { method: 'POST' }); load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 8px; }
.glass-input { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--glass-border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.empty { text-align: center; padding: 20px; color: var(--text-3); font-size: 13px; }
.section-label { font-size: 11px; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 8px; }
.chip-list { display: flex; flex-wrap: wrap; gap: 8px; }
.chip { display: flex; align-items: center; gap: 6px; padding: 6px 12px; background: var(--surface-hover); border-radius: 20px; font-size: 13px; font-weight: 500; }
.chip-priv { background: rgba(99,102,241,0.1); color: var(--primary); }
.chip-close { background: none; border: none; cursor: pointer; color: var(--text-3); font-size: 12px; padding: 0; line-height: 1; }
.chip-close:hover { color: var(--danger); }
</style>