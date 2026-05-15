<template>
  <div>
    <h3>工作记忆</h3>
    <div class="toolbar">
      <select v-model="selectedGroup" @change="loadEntries">
        <option value="">全部群组 ({{ Object.keys(groups).length }})</option>
        <option v-for="(g, gid) in groups" :key="gid" :value="gid">群 {{ gid }} ({{ (g.entries||[]).length }})</option>
      </select>
      <input v-model="search" placeholder="搜索消息内容..." class="search-input" />
    </div>
    <div v-if="!filtered.length" class="empty">📭 暂无消息</div>
    <table v-else>
      <thead><tr><th>群</th><th>用户</th><th>内容</th><th>时间</th><th>已回复</th><th>操作</th></tr></thead>
      <tbody>
        <tr v-for="(e, i) in filtered" :key="i">
          <td class="mono">{{ e.gid }}</td>
          <td class="mono">{{ e.user_id }}</td>
          <td class="truncate">{{ e.content }}</td>
          <td class="mono">{{ fmtTime(e.timestamp) }}</td>
          <td>{{ e.bot_replied ? '✅' : '❌' }}</td>
          <td><button class="btn btn-danger btn-sm" @click="del(e.gid, e.idx)">删除</button></td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'
const groups = ref({})
const entries = ref([])
const selectedGroup = ref('')
const search = ref('')
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
const filtered = computed(() => {
  if (!search.value) return entries.value
  const q = search.value.toLowerCase()
  return entries.value.filter(e => (e.content || '').toLowerCase().includes(q))
})
async function load() { const d = await api('/api/working-memory'); groups.value = d.groups || {}; loadEntries() }
function loadEntries() {
  const gid = selectedGroup.value
  if (gid) { entries.value = (groups.value[gid]?.entries || []).map((e, i) => ({ ...e, gid, idx: i })) }
  else { const all = []; for (const [gid, g] of Object.entries(groups.value)) { (g.entries || []).forEach((e, i) => all.push({ ...e, gid, idx: i })) }; entries.value = all }
}
async function del(gid, i) { if (!confirm('确定删除？')) return; await api(`/api/working-memory/${gid}/${i}`, { method: 'DELETE' }); load() }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; align-items: center; flex-wrap: wrap; }
.search-input { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; flex: 1; max-width: 260px; }
.search-input:focus { border-color: var(--accent); }
.toolbar select { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
tr:hover { background: var(--surface2); }
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-sm { padding: 5px 12px; font-size: 11px; }
</style>
