<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-if="quota.enabled !== undefined">
        <div class="stat-value" :style="{ color: quota.enabled ? 'var(--success)' : 'var(--text-3)' }">{{ quota.enabled ? '已启用' : '已禁用' }}</div>
        <div class="stat-label">配额系统</div>
        <div class="stat-sub">{{ quota.segment_minutes || 5 }} 分钟/段</div>
      </div>
    </div>

    <div class="card">
      <div class="card-header"><h3>用户兴趣分数</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
      <div v-if="!userList.length" class="empty">暂无用户数据</div>
      <div v-else class="user-grid">
        <div v-for="u in userList" :key="u.uid" class="user-card">
          <div class="user-id mono">{{ u.uid }}</div>
          <div class="user-scores">
            <div class="score-row"><label>兴趣</label><div class="bar-wrap"><div class="bar" :style="{ width: Math.min((u.score || 0) * 100, 100) + '%' }"></div></div><span class="mono">{{ (u.score || 0).toFixed(2) }}</span></div>
            <div class="score-row"><label>标记</label><span class="mono">{{ u.marked_count || 0 }}</span></div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const quota = ref({})
const users = ref({})

const userList = computed(() => {
  return Object.entries(users.value).map(([uid, v]) => ({ uid, ...v }))
})

async function load() {
  try {
    const d = await api('/api/quota')
    quota.value = { enabled: d.enabled, segment_minutes: d.segment_minutes, segments: d.segments || [] }
    users.value = d.users || {}
  } catch {}
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 16px; margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.stat-value { font-size: 24px; font-weight: 700; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.user-grid { display: flex; flex-direction: column; gap: 6px; }
.user-card { padding: 12px; border-radius: var(--radius-sm); background: var(--surface-hover); }
.user-id { font-size: 13px; font-weight: 600; margin-bottom: 6px; }
.user-scores { display: flex; flex-direction: column; gap: 4px; }
.score-row { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.score-row label { width: 36px; color: var(--text-2); }
.bar-wrap { flex: 1; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; }
.bar { height: 100%; background: var(--primary); border-radius: 3px; transition: width 0.5s ease; }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); min-width: 40px; text-align: right; }
</style>