<template>
  <div>
    <h2>💬 对话管理</h2>

    <!-- 统计卡片 -->
    <div class="stats-row">
      <div class="stat-card">
        <div class="stat-num">{{ groups.length }}</div>
        <div class="stat-label">活跃群聊</div>
      </div>
      <div class="stat-card">
        <div class="stat-num">{{ users.length }}</div>
        <div class="stat-label">活跃私聊</div>
      </div>
    </div>

    <!-- 群聊管理 -->
    <div class="section">
      <div class="section-header">
        <h3>👥 群聊</h3>
        <button class="btn btn-sm btn-outline" @click="load">🔄 刷新</button>
      </div>
      <div class="toolbar">
        <input v-model="newGroupId" placeholder="输入群号" @keydown.enter="addGroup" />
        <button class="btn btn-primary" @click="addGroup" :disabled="!newGroupId">➕ 开启群聊</button>
      </div>
      <div v-if="groups.length === 0" class="empty">
        <span>暂无活跃群聊</span>
        <span class="empty-hint">输入群号开启</span>
      </div>
      <div v-else class="item-list">
        <div v-for="gid in groups" :key="gid" class="item-row">
          <div class="item-info">
            <span class="item-icon">👥</span>
            <span class="item-id">群 {{ gid }}</span>
          </div>
          <button class="btn btn-sm btn-danger" @click="toggleGroup(gid, false)">关闭</button>
        </div>
      </div>
    </div>

    <!-- 私聊管理 -->
    <div class="section">
      <div class="section-header">
        <h3>👤 私聊</h3>
      </div>
      <div class="toolbar">
        <input v-model="newUserId" placeholder="输入 QQ 号" @keydown.enter="addUser" />
        <button class="btn btn-primary" @click="addUser" :disabled="!newUserId">➕ 开启私聊</button>
      </div>
      <div v-if="users.length === 0" class="empty">
        <span>暂无活跃私聊</span>
        <span class="empty-hint">输入 QQ 号开启，或在配置页设置自动开启用户</span>
      </div>
      <div v-else class="item-list">
        <div v-for="uid in users" :key="uid" class="item-row">
          <div class="item-info">
            <span class="item-icon">👤</span>
            <span class="item-id">用户 {{ uid }}</span>
          </div>
          <button class="btn btn-sm btn-danger" @click="toggleUser(uid, false)">关闭</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const groups = ref([])
const users = ref([])
const newGroupId = ref('')
const newUserId = ref('')

async function load() {
  try {
    const data = await api('/api/conversations')
    groups.value = (data.groups || []).sort((a, b) => a - b)
    users.value = (data.private_users || []).sort((a, b) => a - b)
  } catch (e) {
    console.error('Failed to load conversations:', e)
  }
}

async function toggleGroup(gid, enable) {
  if (!enable && !confirm(`确定关闭群 ${gid}？`)) return
  try {
    await api(`/api/conversations/group/${gid}/${enable ? 'enable' : 'disable'}`, { method: 'POST' })
    await load()
  } catch (e) { alert('操作失败: ' + e.message) }
}

async function toggleUser(uid, enable) {
  if (!enable && !confirm(`确定关闭用户 ${uid} 的私聊？`)) return
  try {
    await api(`/api/conversations/private/${uid}/${enable ? 'enable' : 'disable'}`, { method: 'POST' })
    await load()
  } catch (e) { alert('操作失败: ' + e.message) }
}

async function addGroup() {
  const gid = parseInt(newGroupId.value)
  if (!gid) return
  try {
    await api(`/api/conversations/group/${gid}/enable`, { method: 'POST' })
    newGroupId.value = ''
    await load()
  } catch (e) { alert('操作失败: ' + e.message) }
}

async function addUser() {
  const uid = parseInt(newUserId.value)
  if (!uid) return
  try {
    await api(`/api/conversations/private/${uid}/enable`, { method: 'POST' })
    newUserId.value = ''
    await load()
  } catch (e) { alert('操作失败: ' + e.message) }
}

onMounted(load)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }

.stats-row { display: flex; gap: 12px; margin-bottom: 16px; }
.stat-card {
  flex: 1; background: #fff; border-radius: var(--radius); padding: 16px 20px;
  box-shadow: var(--shadow); text-align: center;
}
.stat-num { font-size: 28px; font-weight: 700; color: var(--accent); }
.stat-label { font-size: 12px; color: var(--text-dim); margin-top: 4px; }

.section {
  background: #fff; border-radius: var(--radius); padding: 16px;
  margin-bottom: 12px; box-shadow: var(--shadow);
}

.section-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
.section-header h3 { margin: 0; font-size: 14px; font-weight: 600; }

.toolbar { display: flex; gap: 8px; margin-bottom: 14px; }
.toolbar input {
  background: var(--accent-light); border: 1.5px solid var(--border); color: var(--text);
  padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; width: 180px;
}
.toolbar input:focus { border-color: var(--accent); }

.empty {
  text-align: center; padding: 24px 0; color: var(--text-dim); font-size: 13px;
  display: flex; flex-direction: column; gap: 4px;
}
.empty-hint { font-size: 11px; opacity: 0.7; }

.item-list { display: flex; flex-direction: column; gap: 6px; }
.item-row {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 14px; background: var(--accent-light); border-radius: var(--radius);
  transition: background .1s;
}
.item-row:hover { background: var(--border); }
.item-info { display: flex; align-items: center; gap: 10px; }
.item-icon { font-size: 18px; }
.item-id { font-weight: 500; font-size: 14px; }

.btn {
  border: none; padding: 8px 16px; border-radius: var(--radius);
  cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s;
}
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background: var(--accent); color: #fff; }
.btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: transparent; border: 1.5px solid var(--border); color: var(--text); }
.btn-sm { padding: 5px 12px; font-size: 11px; }
</style>
