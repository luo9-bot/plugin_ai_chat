<template>
  <div>
    <div class="glass-card">
      <div class="card-header">
        <h3>用户记忆 <span class="badge">三层文件存储</span></h3>
        <div class="header-actions">
          <input v-model="search" placeholder="搜索..." class="glass-input" />
          <select v-model="selectedUser" @change="loadEntries" class="glass-select">
            <option value="">全部用户 ({{ Object.keys(users).length }})</option>
            <option v-for="(u, uid) in users" :key="uid" :value="uid">用户 {{ uid }} ({{ (u.entries||[]).length }})</option>
          </select>
          <button class="btn btn-primary" @click="showAdd = true">＋ 添加</button>
          <a class="btn btn-ghost" href="/api/memory/export" target="_blank">📥 导出</a>
        </div>
      </div>
      <div v-if="!filtered.length" class="empty">📭 暂无记忆</div>
      <div v-else class="table-wrap">
        <table>
          <thead><tr><th>用户</th><th>内容</th><th>重要性</th><th>创建时间</th><th>操作</th></tr></thead>
          <tbody>
            <tr v-for="(e, i) in filtered" :key="i">
              <td class="mono">{{ e.uid }}</td>
              <td class="truncate">{{ e.content }}</td>
              <td><span :class="'tag tag-' + impClass(e.importance)">{{ impLabel(e.importance) }}</span></td>
              <td class="mono">{{ fmtTime(e.created) }}</td>
              <td class="actions">
                <button class="btn btn-ghost btn-sm" @click="del(e.uid, e.idx)">删除</button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="glass-card">
      <div class="card-header"><h3>数据存储结构</h3></div>
      <div class="file-tree">
        <div class="tree-item dir">📁 memory/</div>
        <div class="tree-item dir" style="padding-left:24px">📁 users/<span class="tree-meta"> ← 全局记忆</span></div>
        <div class="tree-item file" style="padding-left:48px">📄 512166443.json</div>
        <div class="tree-item dir" style="padding-left:24px">📁 groups/<span class="tree-meta"> ← 群隔离记忆</span></div>
        <div class="tree-item dir" style="padding-left:48px">📁 676426335/</div>
        <div class="tree-item file" style="padding-left:72px">📄 group.json</div>
        <div class="tree-item file" style="padding-left:72px">📄 512166443.json</div>
      </div>
    </div>

    <div v-if="showAdd" class="modal-overlay" @click.self="showAdd = false">
      <div class="glass-card modal">
        <h3 style="margin-bottom:16px">添加用户记忆</h3>
        <label>用户 ID</label><input v-model="addUid" class="glass-input" style="margin-bottom:12px" />
        <label>内容</label><textarea v-model="addContent" class="glass-input" style="margin-bottom:12px"></textarea>
        <label>重要性</label>
        <select v-model="addImp" class="glass-select" style="margin-bottom:16px">
          <option value="Normal">普通</option><option value="Important">重要</option><option value="Permanent">永久</option>
        </select>
        <div class="modal-actions"><button class="btn btn-ghost" @click="showAdd = false">取消</button><button class="btn btn-primary" @click="addMem">保存</button></div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const users = ref({})
const entries = ref([])
const selectedUser = ref('')
const search = ref('')
const showAdd = ref(false)
const addUid = ref('')
const addContent = ref('')
const addImp = ref('Normal')

function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
function impClass(imp) { const s = typeof imp === 'string' ? imp : Object.keys(imp || {})[0] || 'Normal'; return s.toLowerCase() }
function impLabel(imp) { const s = typeof imp === 'string' ? imp : Object.keys(imp || {})[0] || 'Normal'; return s }

const filtered = computed(() => {
  let list = entries.value
  if (search.value) { const q = search.value.toLowerCase(); list = list.filter(e => (e.content || '').toLowerCase().includes(q)) }
  return list
})

async function load() { const d = await api('/api/memory'); users.value = d.users || {}; loadEntries() }
function loadEntries() {
  const uid = selectedUser.value
  if (uid) { const u = users.value[uid]; entries.value = (u?.entries || []).map((e, i) => ({ ...e, uid, idx: i })) }
  else { const all = []; for (const [uid, u] of Object.entries(users.value)) { (u.entries || []).forEach((e, i) => all.push({ ...e, uid, idx: i })) }; entries.value = all }
}
async function del(uid, idx) { await api(`/api/memory/${uid}/delete`, { method: 'POST', body: JSON.stringify({ index: idx }) }); load() }
async function addMem() { if (!addUid.value || !addContent.value) return; await api(`/api/memory/${addUid.value}/add`, { method: 'POST', body: JSON.stringify({ content: addContent.value, importance: addImp.value }) }); showAdd.value = false; load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.glass-input, .glass-select { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--glass-border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.glass-input:focus, .glass-select:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-glow); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.5px; border-bottom: 1px solid var(--glass-border); }
td { padding: 8px 12px; border-bottom: 1px solid var(--glass-border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.truncate { max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { font-size: 11px; padding: 2px 8px; border-radius: 4px; font-weight: 500; }
.tag-permanent { background: #fef3c7; color: #92400e; }
.tag-important { background: #dbeafe; color: #1e40af; }
.tag-normal { background: var(--surface); color: var(--text-2); }
[data-theme="dark"] .tag-permanent { background: rgba(251,191,36,0.2); color: #fbbf24; }
[data-theme="dark"] .tag-important { background: rgba(96,165,250,0.2); color: #60a5fa; }
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); z-index: 200; display: flex; align-items: center; justify-content: center; }
.modal { width: 420px; padding: 24px; }
.modal label { display: block; font-size: 12px; font-weight: 600; margin-bottom: 4px; color: var(--text-2); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; }
.file-tree { font-family: monospace; font-size: 12px; line-height: 1.8; }
.tree-item { padding: 2px 0; }
.tree-item.dir { color: var(--primary); font-weight: 600; }
.tree-item.file { color: var(--text-2); }
.tree-meta { color: var(--text-3); font-size: 11px; font-weight: 400; }
</style>