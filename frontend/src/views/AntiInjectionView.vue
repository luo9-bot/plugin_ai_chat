<template>
  <div>
    <div class="glass-card">
      <div class="card-header">
        <h3>防注入系统</h3>
        <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
      </div>
      <div v-if="!users.length" class="empty">暂无用户风险数据</div>
      <div v-else class="table-wrap">
        <table>
          <thead><tr><th>用户 ID</th><th>信誉评分</th><th>违规次数</th><th>静默封禁</th><th>识图禁用</th><th>回复惩罚</th><th>操作</th></tr></thead>
          <tbody>
            <tr v-for="u in users" :key="u.user_id">
              <td class="mono">{{ u.user_id }}</td>
              <td><div class="bar-wrap" style="width:80px"><div class="bar" :style="{ width: Math.min((u.reputation || 0) * 100, 100) + '%', background: u.reputation > 0.6 ? 'var(--success)' : u.reputation > 0.3 ? 'var(--warning)' : 'var(--danger)' }"></div></div></td>
              <td class="mono">{{ u.violation_count || 0 }}</td>
              <td><span class="badge" :class="u.silent_banned ? 'badge-danger' : 'badge-safe'">{{ u.silent_banned ? '是' : '否' }}</span></td>
              <td><span class="badge" :class="u.vision_disabled ? 'badge-warn' : 'badge-safe'">{{ u.vision_disabled ? '是' : '否' }}</span></td>
              <td class="mono">{{ u.penalty_multiplier ? (u.penalty_multiplier).toFixed(2) + 'x' : '-' }}</td>
              <td class="actions">
                <button class="btn btn-ghost btn-xs" @click="unban(u.user_id)" v-if="u.silent_banned">解封</button>
                <button class="btn btn-ghost btn-xs" @click="unbanVision(u.user_id)" v-if="u.vision_disabled">恢复识图</button>
                <button class="btn btn-ghost btn-xs" @click="resetRep(u.user_id)">重置信誉</button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const users = ref([])

async function load() {
  try {
    const d = await api('/api/anti-injection/users')
    users.value = d.users || []
    // Enrich with status info
    for (const u of users.value) {
      try {
        const detail = await api('/api/anti-injection/' + u.user_id)
        Object.assign(u, detail)
      } catch {}
    }
  } catch {}
}
async function unban(uid) { await api('/api/anti-injection/' + uid + '/unban', { method: 'POST' }); load() }
async function unbanVision(uid) { await api('/api/anti-injection/' + uid + '/enable-vision', { method: 'POST' }); load() }
async function resetRep(uid) { await api('/api/anti-injection/' + uid + '/reset-reputation', { method: 'POST' }); load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--glass-border); }
td { padding: 8px 12px; border-bottom: 1px solid var(--glass-border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.bar-wrap { height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; }
.bar { height: 100%; border-radius: 3px; transition: width 0.5s ease; }
.actions { display: flex; gap: 4px; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 4px; }
.badge-danger { background: rgba(239,68,68,0.15); color: var(--danger); }
.badge-safe { background: rgba(52,211,153,0.15); color: var(--success); }
.badge-warn { background: rgba(251,191,36,0.15); color: var(--warning); }
</style>