<template>
  <div>
    <h3>自我记忆 <span class="count">({{ thoughts.length }})</span></h3>
    <div class="toolbar">
      <input v-model="search" placeholder="搜索记忆内容..." class="search-input" />
      <input type="date" v-model="dateFrom" class="date-input" />
      <span class="sep">~</span>
      <input type="date" v-model="dateTo" class="date-input" />
      <button class="btn btn-primary" @click="showAdd = true">＋ 添加</button>
      <button v-if="selected.length" class="btn btn-danger btn-sm" @click="batchDelete">🗑 删除 {{ selected.length }} 条</button>
      <a class="btn btn-export btn-sm" href="/api/self-thoughts/export" target="_blank">📥 导出</a>
    </div>
    <div v-if="!filtered.length" class="empty">💭 暂无记忆</div>
    <table v-else>
      <thead><tr><th><input type="checkbox" @change="toggleAll" :checked="allSelected" class="cb" /></th><th>分类</th><th>内容</th><th>时间</th><th>操作</th></tr></thead>
      <tbody>
        <tr v-for="(t, i) in filtered" :key="i" :class="{ selected: selected.includes(t._idx) }">
          <td><input type="checkbox" :value="t._idx" v-model="selected" class="cb" /></td>
          <td><span class="tag">{{ catIcon[t.category] }} {{ t.category }}</span></td>
          <td class="truncate">{{ t.content }}</td>
          <td class="mono">{{ fmtTime(t.created) }}</td>
          <td class="actions">
            <button class="btn btn-outline btn-sm" @click="editIdx = t._idx; editContent = t.content; editCategory = t.category">编辑</button>
            <button class="btn btn-danger btn-sm" @click="del(t._idx)">删除</button>
          </td>
        </tr>
      </tbody>
    </table>
    <!-- Add Modal -->
    <div v-if="showAdd" class="modal-overlay" @click.self="showAdd = false">
      <div class="modal"><h3>＋ 添加自我记忆</h3>
        <label>分类</label><select v-model="addCategory"><option value="reflection">💭 反思</option><option value="experience">💼 经历</option><option value="plan">📝 计划</option><option value="feeling">💗 感受</option></select>
        <label>内容</label><textarea v-model="addContent" placeholder="输入记忆内容..."></textarea>
        <div class="modal-actions"><button class="btn btn-outline" @click="showAdd = false">取消</button><button class="btn btn-primary" @click="addThought">保存</button></div>
      </div>
    </div>
    <!-- Edit Modal -->
    <div v-if="editIdx >= 0" class="modal-overlay" @click.self="editIdx = -1">
      <div class="modal"><h3>✏️ 编辑记忆</h3>
        <label>分类</label><select v-model="editCategory"><option value="reflection">💭 反思</option><option value="experience">💼 经历</option><option value="plan">📝 计划</option><option value="feeling">💗 感受</option></select>
        <label>内容</label><textarea v-model="editContent"></textarea>
        <div class="modal-actions"><button class="btn btn-outline" @click="editIdx = -1">取消</button><button class="btn btn-primary" @click="saveEdit">保存</button></div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const catIcon = { reflection: '💭', experience: '💼', plan: '📝', feeling: '💗' }
const thoughts = ref([])
const search = ref('')
const dateFrom = ref('')
const dateTo = ref('')
const selected = ref([])
const showAdd = ref(false)
const addContent = ref('')
const addCategory = ref('reflection')
const editIdx = ref(-1)
const editContent = ref('')
const editCategory = ref('reflection')

function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

const filtered = computed(() => {
  let list = thoughts.value.map((t, i) => ({ ...t, _idx: i }))
  if (search.value) { const q = search.value.toLowerCase(); list = list.filter(t => (t.content || '').toLowerCase().includes(q)) }
  if (dateFrom.value) { const ts = new Date(dateFrom.value).getTime() / 1000; list = list.filter(t => (t.created || 0) >= ts) }
  if (dateTo.value) { const ts = new Date(dateTo.value).getTime() / 1000 + 86400; list = list.filter(t => (t.created || 0) < ts) }
  return list
})
const allSelected = computed(() => filtered.value.length > 0 && filtered.value.every(t => selected.value.includes(t._idx)))
function toggleAll(e) { selected.value = e.target.checked ? filtered.value.map(t => t._idx) : [] }

async function load() { const d = await api('/api/self-thoughts'); thoughts.value = d.thoughts || []; selected.value = [] }
async function addThought() {
  if (!addContent.value.trim()) return
  await api('/api/self-thoughts', { method: 'POST', body: JSON.stringify({ content: addContent.value.trim(), category: addCategory.value }) })
  showAdd.value = false; addContent.value = ''; load()
}
async function saveEdit() {
  if (!editContent.value.trim()) return
  await api('/api/self-thoughts/' + editIdx.value, { method: 'PUT', body: JSON.stringify({ content: editContent.value.trim(), category: editCategory.value }) })
  editIdx.value = -1; load()
}
async function del(i) { if (!confirm('确定删除？')) return; await api('/api/self-thoughts/' + i, { method: 'DELETE' }); load() }
async function batchDelete() {
  if (!confirm(`确定删除选中的 ${selected.value.length} 条记忆？`)) return
  await api('/api/self-thoughts/batch', { method: 'POST', body: JSON.stringify({ indices: selected.value }) })
  load()
}
onMounted(load)
</script>

<style scoped>
.count { color: var(--text-dim); font-size: 14px; }
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; align-items: center; flex-wrap: wrap; }
.search-input { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; flex: 1; min-width: 150px; }
.search-input:focus { border-color: var(--accent); }
.date-input { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; width: 130px; }
.date-input:focus { border-color: var(--accent); }
.sep { color: var(--text-dim); }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--border); }
th { background: #f9fafb; color: var(--text-dim); font-weight: 600; font-size: 12px; }
tr:hover { background: #f9fafb; }
tr.selected { background: var(--accent-light); }
.tag { display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; background: var(--accent-light); color: var(--accent); }
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.cb { width: 16px; height: 16px; accent-color: var(--accent); cursor: pointer; }
.actions { display: flex; gap: 6px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn:hover { opacity: 0.9; }
.btn-primary { background: var(--accent); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: var(--surface); border: 1.5px solid var(--border); color: var(--accent); }
.btn-export { background: var(--surface); color: var(--accent); border: 1.5px solid var(--border); text-decoration: none; }
.btn-sm { padding: 5px 12px; font-size: 11px; }
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,.4); display: flex; align-items: center; justify-content: center; z-index: 100; }
.modal { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 24px; width: 500px; max-width: 90vw; max-height: 80vh; overflow-y: auto; box-shadow: 0 10px 40px rgba(0,0,0,.15); }
.modal h3 { margin-top: 0; font-size: 16px; color: var(--text); }
.modal label { display: block; margin: 14px 0 4px; font-size: 12px; color: var(--text-dim); font-weight: 500; }
.modal input, .modal select, .modal textarea { width: 100%; background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 10px 14px; border-radius: var(--radius); font-size: 13px; font-family: inherit; outline: none; transition: border .2s; }
.modal textarea { min-height: 80px; resize: vertical; }
.modal input:focus, .modal select:focus, .modal textarea:focus { border-color: var(--accent); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 24px; }

/* Mobile */
@media (max-width: 768px) {
  .toolbar { flex-direction: column; align-items: stretch; }
  .search-input { max-width: none; }
  .date-input { width: 100%; }
  table { font-size: 12px; }
  th, td { padding: 8px 10px; }
  .truncate { max-width: 150px; }
  .modal { padding: 16px; }
}
</style>
