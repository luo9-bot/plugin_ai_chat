<template>
  <div>
    <h2>💬 对话管理</h2>

    <!-- 群聊管理 -->
    <div class="section">
      <h3>👥 群聊管理</h3>
      <div class="toolbar">
        <input v-model="newGroupId" placeholder="输入群号" style="width:160px" @keydown.enter="addGroup" />
        <button class="btn btn-primary" @click="addGroup">开启群聊</button>
        <button class="btn btn-outline" @click="load">刷新</button>
      </div>
      <div v-if="groups.length === 0" class="empty">当前没有开启的群聊</div>
      <div v-else class="item-list">
        <div v-for="gid in groups" :key="gid" class="item-row">
          <span class="item-id">群 {{ gid }}</span>
          <button class="btn btn-danger btn-sm" @click="toggleGroup(gid, false)">关闭</button>
        </div>
      </div>
    </div>

    <!-- 私聊管理 -->
    <div class="section">
      <h3>👤 私聊管理</h3>
      <div class="toolbar">
        <input v-model="newUserId" placeholder="输入 QQ 号" style="width:160px" @keydown.enter="addUser" />
        <button class="btn btn-primary" @click="addUser">开启私聊</button>
      </div>
      <div v-if="users.length === 0" class="empty">当前没有开启的私聊</div>
      <div v-else class="item-list">
        <div v-for="uid in users" :key="uid" class="item-row">
          <span class="item-id">用户 {{ uid }}</span>
          <button class="btn btn-danger btn-sm" @click="toggleUser(uid, false)">关闭</button>
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
    groups.value = data.groups || []
    users.value = data.private_users || []
  } catch (e) {
    console.error('Failed to load conversations:', e)
  }
}

async function toggleGroup(gid, enable) {
  const action = enable ? 'enable' : 'disable'
  if (!enable && !confirm(`确定关闭群 ${gid} 的对话？`)) return
  try {
    await api(`/api/conversations/group/${gid}/${action}`, { method: 'POST' })
    await load()
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

async function toggleUser(uid, enable) {
  const action = enable ? 'enable' : 'disable'
  if (!enable && !confirm(`确定关闭用户 ${uid} 的私聊？`)) return
  try {
    await api(`/api/conversations/private/${uid}/${action}`, { method: 'POST' })
    await load()
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

async function addGroup() {
  const gid = parseInt(newGroupId.value)
  if (!gid) return
  try {
    await api(`/api/conversations/group/${gid}/enable`, { method: 'POST' })
    newGroupId.value = ''
    await load()
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

async function addUser() {
  const uid = parseInt(newUserId.value)
  if (!uid) return
  try {
    await api(`/api/conversations/private/${uid}/enable`, { method: 'POST' })
    newUserId.value = ''
    await load()
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

onMounted(load)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin-bottom: 12px; font-weight: 600; color: var(--text); }

.section {
  background: #fff;
  border-radius: var(--radius);
  padding: 16px;
  margin-bottom: 16px;
  box-shadow: var(--shadow);
}

.toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
  align-items: center;
}

.toolbar input {
  background: #fff;
  border: 1.5px solid var(--border);
  color: var(--text);
  padding: 8px 12px;
  border-radius: var(--radius);
  font-size: 13px;
  outline: none;
}

.toolbar input:focus {
  border-color: var(--accent);
}

.empty {
  color: var(--text-dim);
  font-size: 13px;
  padding: 12px 0;
}

.item-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.item-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: var(--accent-light);
  border-radius: var(--radius);
}

.item-id {
  font-weight: 500;
  font-size: 14px;
}

.btn {
  border: none;
  padding: 8px 16px;
  border-radius: var(--radius);
  cursor: pointer;
  font-size: 12px;
  font-weight: 500;
  transition: all .15s;
}

.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background: var(--accent); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: transparent; border: 1.5px solid var(--border); color: var(--text); }
.btn-sm { padding: 5px 12px; font-size: 11px; }
</style>
