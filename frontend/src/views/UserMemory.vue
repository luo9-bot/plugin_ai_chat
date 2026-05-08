<template>
  <div>
    <h2>📦 用户记忆</h2>
    <div class="toolbar">
      <select v-model="selectedUser" @change="loadEntries">
        <option value="">全部用户 ({{ Object.keys(users).length }})</option>
        <option v-for="(u, uid) in users" :key="uid" :value="uid">用户 {{ uid }} ({{ (u.entries||[]).length }})</option>
      </select>
      <input v-model="search" placeholder="搜索记忆内容..." class="search-input" />
      <input type="date" v-model="dateFrom" class="date-input" />
      <span class="sep">~</span>
      <input type="date" v-model="dateTo" class="date-input" />
      <button class="btn btn-primary" @click="showAdd = true">＋ 添加</button>
      <button v-if="selected.length" class="btn btn-danger btn-sm" @click="batchDelete">🗑 删除 {{ selected.length }} 条</button>
      <a class="btn btn-export btn-sm" href="/api/memory/export" target="_blank">📥 导出</a>
    </div>
    <div v-if="!filtered.length" class="empty">📭 暂无记忆</div>
    <table v-else>
      <thead><tr><th><input type="checkbox" @change="toggleAll" :checked="allSelected" class="cb" /></th><th>用户</th><th>内容</th><th>重要性</th><th>创建</th><th>访问</th><th>次数</th><th>操作</th></tr></thead>
      <tbody>
        <tr v-for="(e, i) in filtered" :key="i" :class="{ selected: selected.includes(e.uid + ':' + e.idx) }">
          <td><input type="checkbox" :value="e.uid + ':' + e.idx" v-model="selected" class="cb" /></td>
          <td class="mono">{{ e.uid }}</td>
          <td class="truncate">{{ e.content }}</td>
          <td><span :class="'badge badge-' + impClass(e.importance)">{{ impLabel(e.importance) }}</span></td>
          <td class="mono">{{ fmtTime(e.created) }}</td>
          <td class="mono">{{ fmtTime(e.last_accessed) }}</td>
          <td>{{ e.access_count || 0 }}</td>
          <td class="actions">
            <button class="btn btn-outline btn-sm" @click="startEdit(e)">编辑</button>
            <button class="btn btn-danger btn-sm" @click="del(e.uid, e.idx)">删除</button>
          </td>
        </tr>
      </tbody>
    </table>
    <!-- Add Modal -->
    <div v-if="showAdd" class="modal-overlay" @click.self="showAdd = false">
      <div class="modal"><h3>＋ 添加用户记忆</h3>
        <label>用户 ID (QQ号)</label><input v-model="addUid" placeholder="如: 123456789" />
        <label>内容</label><textarea v-model="addContent" placeholder="记忆内容..."></textarea>
        <label>重要性</label><select v-model="addImp"><option value="Normal">💜 普通</option><option value="Important">⭐ 重要</option><option value="Permanent">💎 永久</option></select>
        <div class="modal-actions"><button class="btn btn-outline" @click="showAdd = false">取消</button><button class="btn btn-primary" @click="addMem">保存</button></div>
      </div>
    </div>
    <!-- Edit Modal -->
    <div v-if="editData" class="modal-overlay" @click.self="editData = null">
      <div class="modal"><h3>✏️ 编辑记忆</h3>
        <label>内容</label><textarea v-model="editContent"></textarea>
        <label>重要性</label><select v-model="editImp"><option value="Normal">💜 普通</option><option value="Important">⭐ 重要</option><option value="Permanent">💎 永久</option></select>
        <div class="modal-actions"><button class="btn btn-outline" @click="editData = null">取消</button><button class="btn btn-primary" @click="saveEdit">保存</button></div>
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
const dateFrom = ref('')
const dateTo = ref('')
const selected = ref([])
const showAdd = ref(false)
const addUid = ref('')
const addContent = ref('')
const addImp = ref('Normal')
const editData = ref(null)
const editContent = ref('')
const editImp = ref('Normal')

function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
function impClass(imp) { const s = typeof imp === 'string' ? imp : Object.keys(imp || {})[0] || 'Normal'; return s.toLowerCase() }
function impLabel(imp) { const s = typeof imp === 'string' ? imp : Object.keys(imp || {})[0] || 'Normal'; return s }

const filtered = computed(() => {
  let list = entries.value
  if (search.value) { const q = search.value.toLowerCase(); list = list.filter(e => (e.content || '').toLowerCase().includes(q)) }
  if (dateFrom.value) { const ts = new Date(dateFrom.value).getTime() / 1000; list = list.filter(e => (e.created || 0) >= ts) }
  if (dateTo.value) { const ts = new Date(dateTo.value).getTime() / 1000 + 86400; list = list.filter(e => (e.created || 0) < ts) }
  return list
})
const allSelected = computed(() => filtered.value.length > 0 && filtered.value.every(e => selected.value.includes(e.uid + ':' + e.idx)))
function toggleAll(e) { selected.value = e.target.checked ? filtered.value.map(x => x.uid + ':' + x.idx) : [] }

async function load() { const d = await api('/api/memory'); users.value = d.users || {}; loadEntries() }
function loadEntries() {
  const uid = selectedUser.value
  if (uid) {
    const u = users.value[uid]
    entries.value = (u?.entries || []).map((e, i) => ({ ...e, uid, idx: i }))
  } else {
    const all = []
    for (const [uid, u] of Object.entries(users.value)) { (u.entries || []).forEach((e, i) => all.push({ ...e, uid, idx: i })) }
    entries.value = all
  }
  selected.value = []
}
async function addMem() {
  if (!addUid.value.trim() || !addContent.value.trim()) return
  await api('/api/memory/' + addUid.value.trim(), { method: 'POST', body: JSON.stringify({ content: addContent.value.trim(), importance: addImp.value }) })
  showAdd.value = false; addContent.value = ''; load()
}
function startEdit(e) { editData.value = e; editContent.value = e.content; editImp.value = impLabel(e.importance) }
async function saveEdit() {
  if (!editContent.value.trim() || !editData.value) return
  await api(`/api/memory/${editData.value.uid}/${editData.value.idx}`, { method: 'PUT', body: JSON.stringify({ content: editContent.value.trim(), importance: editImp.value }) })
  editData.value = null; load()
}
async function del(uid, i) { if (!confirm('确定删除？')) return; await api(`/api/memory/${uid}/${i}`, { method: 'DELETE' }); load() }
async function batchDelete() {
  if (!confirm(`确定删除选中的 ${selected.value.length} 条记忆？`)) return
  const byUid = {}
  selected.value.forEach(key => { const [uid, idx] = key.split(':'); if (!byUid[uid]) byUid[uid] = []; byUid[uid].push(parseInt(idx)) })
  for (const [uid, indices] of Object.entries(byUid)) { await api(`/api/memory/${uid}/batch`, { method: 'POST', body: JSON.stringify({ indices }) }) }
  load()
}
onMounted(load)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; align-items: center; flex-wrap: wrap; }
.search-input { background: #fff; border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; flex: 1; max-width: 260px; }
.search-input:focus { border-color: var(--accent); }
.date-input { background: #fff; border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
.date-input:focus { border-color: var(--accent); }
.sep { color: var(--text-dim); }
.toolbar select { background: #fff; border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
.toolbar select:focus { border-color: var(--accent); }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: #fff; border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; letter-spacing: .5px; }
tr:hover { background: var(--surface2); }
tr.selected { background: var(--accent-light); }
.badge { display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; font-weight: 500; }
.badge-permanent { background: #fee2e2; color: #ef4444; }
.badge-important { background: #fef3c7; color: #f59e0b; }
.badge-normal { background: #f3e8ff; color: #a855f7; }
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.cb { width: 16px; height: 16px; accent-color: var(--accent); cursor: pointer; }
.actions { display: flex; gap: 6px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn:hover { transform: translateY(-1px); }
.btn-primary { background: linear-gradient(135deg, var(--accent), var(--purple)); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: #fff; border: 1.5px solid var(--border); color: var(--accent); }
.btn-outline:hover { background: var(--accent-light); }
.btn-export { background: var(--blue-light); color: var(--blue); border: 1.5px solid var(--blue); text-decoration: none; }
.btn-export:hover { background: var(--blue); color: #fff; }
.btn-sm { padding: 5px 12px; font-size: 11px; }
.modal-overlay { position: fixed; inset: 0; background: rgba(74,53,72,.4); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; z-index: 100; }
.modal { background: #fff; border: 2px solid var(--border); border-radius: 16px; padding: 28px; width: 500px; max-width: 90vw; max-height: 80vh; overflow-y: auto; box-shadow: 0 20px 60px rgba(236,72,153,.15); }
.modal h3 { margin-top: 0; font-size: 16px; color: var(--accent); }
.modal label { display: block; margin: 14px 0 4px; font-size: 12px; color: var(--text-dim); font-weight: 500; }
.modal input, .modal select, .modal textarea { width: 100%; background: var(--surface2); border: 1.5px solid var(--border); color: var(--text); padding: 10px 14px; border-radius: var(--radius); font-size: 13px; font-family: inherit; outline: none; transition: border .2s; }
.modal textarea { min-height: 80px; resize: vertical; }
.modal input:focus, .modal select:focus, .modal textarea:focus { border-color: var(--accent); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 24px; }
</style>
