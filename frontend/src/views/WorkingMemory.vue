<template>
  <div>
    <div class="glass-card">
      <div class="card-header">
        <h3>工作记忆 <span class="badge">群聊消息流</span></h3>
        <div class="header-actions">
          <input v-model="filterGroup" placeholder="群号过滤..." class="glass-input" />
          <select v-model="groupSelect" @change="loadData" class="glass-select">
            <option value="">全部群</option>
            <option v-for="g in groupIds" :key="g" :value="g">群 {{ g }}</option>
          </select>
          <button class="btn btn-ghost btn-sm" @click="autoRefresh = !autoRefresh">{{ autoRefresh ? '⏸ 暂停' : '▶ 自动' }}</button>
        </div>
      </div>
      <div v-if="!entries.length" class="empty">暂无工作记忆</div>
      <div v-else class="msg-flow">
        <div v-for="(e, i) in entries" :key="i" class="msg-item" :class="{ isBot: e.is_bot }">
          <div class="msg-avatar" :style="{ background: e.is_bot ? 'var(--primary)' : 'var(--text-3)' }">
            {{ e.is_bot ? 'B' : 'U' }}
          </div>
          <div class="msg-body">
            <div class="msg-header">
              <span class="msg-user" :class="{ bot: e.is_bot }">{{ e.is_bot ? 'Bot' : 'user_id:' + e.user_id }}</span>
              <span class="msg-time">{{ fmtTime(e.timestamp) }}</span>
              <span class="msg-tag" v-if="e.bot_replied">已回复</span>
            </div>
            <div class="msg-text">{{ e.content }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const entries = ref([])
const groupIds = ref([])
const groupSelect = ref('')
const filterGroup = ref('')
const autoRefresh = ref(false)
let interval = null

function fmtTime(ts) { if (!ts) return '-'; const d = new Date(ts * 1000); return d.toLocaleTimeString('zh-CN') }

const filtered = computed(() => {
  let list = entries.value
  if (filterGroup.value) list = list.filter(e => String(e.group_id).includes(filterGroup.value))
  return list
})

async function loadData() {
  try {
    const d = await api('/api/working-memory')
    const groups = d.groups || {}
    groupIds.value = Object.keys(groups)
    if (groupSelect.value) {
      const g = groups[groupSelect.value]
      entries.value = (g?.entries || []).reverse().slice(0, 100).map(e => ({ ...e, is_bot: false }))
    } else {
      const all = []
      for (const [gid, g] of Object.entries(groups)) {
        (g.entries || []).forEach(e => all.push({ ...e, group_id: gid, is_bot: false }))
      }
      entries.value = all.reverse().slice(0, 200)
    }
  } catch {}
}

onMounted(() => { loadData(); interval = setInterval(() => { if (autoRefresh.value) loadData() }, 5000); window.addEventListener('refresh-all', loadData) })
onUnmounted(() => { clearInterval(interval); window.removeEventListener('refresh-all', loadData) })
</script>

<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 8px; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.glass-input, .glass-select { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--glass-border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
.msg-flow { display: flex; flex-direction: column; gap: 8px; max-height: 70vh; overflow-y: auto; }
.msg-item { display: flex; gap: 10px; padding: 8px; border-radius: var(--radius-sm); transition: var(--transition); }
.msg-item:hover { background: var(--surface-hover); }
.msg-avatar { width: 28px; height: 28px; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: #fff; font-size: 12px; font-weight: 700; flex-shrink: 0; }
.msg-body { flex: 1; min-width: 0; }
.msg-header { display: flex; align-items: center; gap: 8px; margin-bottom: 2px; }
.msg-user { font-size: 12px; font-weight: 600; }
.msg-user.bot { color: var(--primary); }
.msg-time { font-size: 11px; color: var(--text-3); }
.msg-tag { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: rgba(52,211,153,0.15); color: var(--success); }
.msg-text { font-size: 13px; line-height: 1.4; word-break: break-word; }
</style>