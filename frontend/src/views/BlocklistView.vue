<template>
  <div>
    <h2>🚫 黑名单</h2>
    <div class="toolbar">
      <input v-model="newUid" placeholder="输入 QQ 号" style="width:160px" @keydown.enter="addBlock" />
      <button class="btn btn-danger" @click="addBlock">🚫 拉黑</button>
    </div>
    <div v-if="!list.length" class="empty">😊 黑名单为空</div>
    <table v-else><thead><tr><th>用户 ID</th><th>操作</th></tr></thead><tbody>
      <tr v-for="u in list" :key="u">
        <td class="mono">{{ u }}</td>
        <td><button class="btn btn-success btn-sm" @click="removeBlock(u)">移除</button></td>
      </tr>
    </tbody></table>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const list = ref([])
const newUid = ref('')
async function load() { const d = await api('/api/blocklist'); list.value = d.blocked || [] }
async function addBlock() { const uid = parseInt(newUid.value); if (!uid) return; await api('/api/blocklist', { method: 'POST', body: JSON.stringify({ user_id: uid }) }); newUid.value = ''; load() }
async function removeBlock(uid) { if (!confirm(`确定将 ${uid} 移出黑名单？`)) return; await api('/api/blocklist/' + uid, { method: 'DELETE' }); load() }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; align-items: center; }
.toolbar input { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
.toolbar input:focus { border-color: var(--accent); }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-success { background: var(--success); color: #fff; }
.btn-sm { padding: 5px 12px; font-size: 11px; }
</style>
