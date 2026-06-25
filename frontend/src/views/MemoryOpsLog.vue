<template>
  <div>
    <div class="card">
      <div class="card-header">
        <h3>内存操作监视 <span class="badge">{{ entries.length }} 条</span></h3>
        <div class="header-actions">
          <select v-model="filterOp" class="glass-select">
            <option value="">全部操作</option>
            <option value="add">添加</option>
            <option value="add_group">添加(群)</option>
            <option value="forget">遗忘</option>
            <option value="forget_all">全部遗忘</option>
            <option value="correct">修正</option>
            <option value="ai_extract">AI 提取</option>
            <option value="keyword_extract">关键词提取</option>
            <option value="auto_summarize">自动摘要</option>
            <option value="review_add">审查添加</option>
            <option value="review_remove">审查删除</option>
            <option value="review_update">审查更新</option>
          </select>
          <input v-model="filterUser" placeholder="用户ID..." class="glass-input" style="width:100px" />
          <button class="btn btn-ghost btn-sm" @click="autoRefresh = !autoRefresh">{{ autoRefresh ? '⏸ 暂停' : '▶ 自动刷新' }}</button>
          <button class="btn btn-ghost btn-sm" @click="load">🔄 刷新</button>
          <button class="btn btn-danger btn-sm" @click="clearLogs">🗑 清空</button>
        </div>
      </div>

      <div v-if="!filtered.length" class="empty">暂无内存操作日志</div>
      <div v-else class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>时间</th>
              <th>操作</th>
              <th>用户</th>
              <th>群</th>
              <th>内容</th>
              <th>重要性</th>
              <th>详情</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(e, i) in filtered" :key="i">
              <td class="mono">{{ fmtTime(e.timestamp) }}</td>
              <td><span :class="'tag tag-' + opClass(e.operation)">{{ opLabel(e.operation) }}</span></td>
              <td class="mono">{{ e.user_id || '-' }}</td>
              <td class="mono">{{ e.group_id || '-' }}</td>
              <td class="truncate" :title="e.content_preview">{{ e.content_preview }}</td>
              <td><span :class="'tag tag-imp-' + (e.importance || 'normal')">{{ e.importance || '-' }}</span></td>
              <td class="detail-cell" :title="e.detail">{{ e.detail }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const entries = ref([])
const filterOp = ref('')
const filterUser = ref('')
const autoRefresh = ref(false)
let interval = null

function fmtTime(ts) {
  if (!ts) return '-'
  return new Date(ts * 1000).toLocaleString('zh-CN')
}

function opClass(op) {
  if (op.startsWith('review')) return 'review'
  if (op.includes('extract') || op.includes('summarize')) return 'ai'
  if (op === 'forget' || op === 'forget_all') return 'forget'
  if (op === 'correct') return 'correct'
  return 'add'
}

function opLabel(op) {
  const map = {
    add: '添加', add_group: '添加(群)', forget: '遗忘', forget_all: '全部遗忘',
    correct: '修正', ai_extract: 'AI提取', keyword_extract: '关键词提取',
    auto_summarize: '自动摘要', review_add: '审查+',
    review_remove: '审查-', review_update: '审查~'
  }
  return map[op] || op
}

const filtered = computed(() => {
  let list = entries.value
  if (filterOp.value) list = list.filter(e => e.operation === filterOp.value)
  if (filterUser.value) list = list.filter(e => String(e.user_id).includes(filterUser.value))
  return list
})

async function load() {
  try {
    const d = await api('/api/memory-ops-log')
    entries.value = (d.entries || []).reverse()
  } catch {}
}

async function clearLogs() {
  if (!confirm('确认清空所有内存操作日志？')) return
  await api('/api/memory-ops-log/clear', { method: 'POST' })
  load()
}

onMounted(() => {
  load()
  interval = setInterval(() => { if (autoRefresh.value) load() }, 3000)
  window.addEventListener('refresh-all', load)
})
onUnmounted(() => {
  clearInterval(interval)
  window.removeEventListener('refresh-all', load)
})
</script>

<style scoped>
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.glass-input, .glass-select { padding: 6px 10px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 12px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-danger { background: var(--danger, #ef4444); color: white; }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 10px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--border); white-space: nowrap; }
td { padding: 7px 10px; border-bottom: 1px solid var(--border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.truncate { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.detail-cell { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--text-2); }
.tag { font-size: 10px; padding: 2px 7px; border-radius: 4px; font-weight: 600; white-space: nowrap; }
.tag-add { background: rgba(52,211,153,0.15); color: #10b981; }
.tag-forget { background: rgba(239,68,68,0.15); color: #ef4444; }
.tag-correct { background: rgba(251,191,36,0.15); color: #f59e0b; }
.tag-ai { background: rgba(96,165,250,0.15); color: #3b82f6; }
.tag-review { background: rgba(168,85,247,0.15); color: #a855f7; }
.tag-imp-permanent { background: #fef3c7; color: #92400e; }
.tag-imp-important { background: #dbeafe; color: #1e40af; }
.tag-imp-normal { background: var(--surface); color: var(--text-2); }
[data-theme="dark"] .tag-imp-permanent { background: rgba(251,191,36,0.2); color: #fbbf24; }
[data-theme="dark"] .tag-imp-important { background: rgba(96,165,250,0.2); color: #60a5fa; }
</style>
