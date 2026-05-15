<template>
  <div>
    <div class="stat-row">
      <div v-for="t in types" :key="t" class="stat-card">
        <div class="stat-num">{{ counts[t] ?? 0 }}</div>
        <div class="stat-label">{{ typeLabel(t) }}</div>
      </div>
    </div>
    <div class="section">
      <div class="toolbar">
        <select v-model="selectedType" @change="loadList" style="width:200px">
          <option v-for="t in types" :key="t" :value="t">{{ typeLabel(t) }} ({{ counts[t] ?? 0 }})</option>
        </select>
        <button class="btn btn-outline btn-sm" @click="load">刷新</button>
      </div>
      <div v-if="backups.length === 0" class="empty">暂无备份文件</div>
      <table v-else>
        <thead><tr><th>文件名</th><th>大小</th><th>操作</th></tr></thead>
        <tbody>
          <tr v-for="b in backups" :key="b.filename">
            <td class="mono">{{ b.filename }}</td>
            <td>{{ (b.size / 1024).toFixed(1) }} KB</td>
            <td><button class="btn btn-success btn-sm" @click="restore(b.filename)">恢复</button></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const types = ref([])
const counts = ref({})
const selectedType = ref('')
const backups = ref([])

const TYPE_LABELS = {
  self_memory: '自我记忆', memory: '用户记忆', working_memory: '工作记忆',
  personality: '人格', emotion: '情绪', mental_state: '心理状态',
  blocklist: '黑名单', proactive: '主动对话', proactive_config: '主动配置', archive: '归档',
}

function typeLabel(t) { return TYPE_LABELS[t] || t }

async function load() {
  try {
    const d = await api('/api/backups')
    types.value = d.types || []
    counts.value = d.counts || {}
    if (types.value.length && !selectedType.value) {
      selectedType.value = types.value[0]
      loadList()
    }
  } catch {}
}

async function loadList() {
  if (!selectedType.value) return
  try {
    const d = await api('/api/backups/' + selectedType.value)
    backups.value = d.backups || []
  } catch {}
}

async function restore(filename) {
  if (!confirm(`确定恢复备份 ${filename}？当前数据将被覆盖（会先备份当前状态）。`)) return
  try {
    await api('/api/backups/restore', { method: 'POST', body: JSON.stringify({ type: selectedType.value, filename }) })
    await load()
  } catch {}
}

onMounted(load)
</script>
<style scoped>
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
</style>
