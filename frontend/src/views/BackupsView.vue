<template>
  <div>
    <h2>💾 备份管理</h2>
    <div class="stat-grid">
      <div class="stat-card" v-for="t in types" :key="t"><div class="label">{{ t }}</div><div class="value">{{ counts[t] || 0 }}</div></div>
    </div>
    <div class="toolbar">
      <select v-model="selectedType" @change="loadList">
        <option v-for="t in types" :key="t" :value="t">{{ t }} ({{ counts[t] || 0 }})</option>
      </select>
    </div>
    <div v-if="!backups.length" class="empty">📭 暂无备份</div>
    <table v-else><thead><tr><th>文件名</th><th>大小</th><th>操作</th></tr></thead><tbody>
      <tr v-for="b in backups" :key="b.filename">
        <td class="mono">{{ b.filename }}</td>
        <td>{{ (b.size / 1024).toFixed(1) }} KB</td>
        <td><button class="btn btn-success btn-sm" @click="restore(b.filename)">恢复</button></td>
      </tr>
    </tbody></table>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const types = ref([]); const counts = ref({}); const selectedType = ref(''); const backups = ref([])
async function load() { const d = await api('/api/backups'); types.value = d.types || []; counts.value = d.counts || {}; if (types.value.length) { selectedType.value = types.value[0]; loadList() } }
async function loadList() { const d = await api('/api/backups/' + selectedType.value); backups.value = d.backups || [] }
async function restore(filename) { if (!confirm(`确定恢复备份 ${filename}？当前数据将被覆盖（会先备份当前状态）。`)) return; await api('/api/backups/restore', { method: 'POST', body: JSON.stringify({ type: selectedType.value, filename }) }); load() }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; }
.toolbar select { background: #fff; border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 12px; margin-bottom: 20px; }
.stat-card { background: #fff; border: 2px solid var(--accent-light); border-radius: var(--radius); padding: 16px; text-align: center; box-shadow: var(--shadow); }
.stat-card .label { font-size: 11px; color: var(--text-dim); margin-bottom: 4px; }
.stat-card .value { font-size: 24px; font-weight: 700; background: linear-gradient(135deg, var(--accent), var(--purple)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: #fff; border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; }
.btn-success { background: var(--success); color: #fff; }
.btn-sm { padding: 5px 12px; font-size: 11px; }
</style>
