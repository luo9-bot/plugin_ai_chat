<template>
  <div>
    <div class="stat-grid">
      <div class="glass-card">
        <div class="stat-value">{{ weekly.done }}/{{ weekly.total }}</div>
        <div class="stat-label">周计划完成</div>
        <div class="bar-wrap"><div class="bar" :style="{ width: weeklyPct + '%' }"></div></div>
      </div>
      <div class="glass-card">
        <div class="stat-value">{{ monthly.done }}/{{ monthly.total }}</div>
        <div class="stat-label">月计划完成</div>
        <div class="bar-wrap"><div class="bar" :style="{ width: monthlyPct + '%' }"></div></div>
      </div>
      <div class="glass-card">
        <div class="stat-value">{{ pushHistory.length }}</div>
        <div class="stat-label">累计推动</div>
        <div class="stat-sub">历史推动记录</div>
      </div>
      <div class="glass-card">
        <div class="stat-value">{{ pushes.length }}</div>
        <div class="stat-label">今日待推进</div>
        <div class="stat-sub">{{ pushState.pushed_today?.length || 0 }} 项已推送</div>
      </div>
    </div>

    <div class="plan-grid">
      <div class="glass-card">
        <div class="card-header">
          <h3>本周计划 <span class="badge" v-if="weekly.week_start">{{ weekly.week_start }}</span></h3>
        </div>
        <div v-if="!weekly.goals?.length" class="empty">暂无周计划</div>
        <div v-else class="goal-list">
          <div v-for="(g, i) in weekly.goals" :key="i" class="goal-item" :class="{ done: g.completed }">
            <div class="goal-check" @click="toggleWeekly(i)">
              <svg v-if="g.completed" viewBox="0 0 20 20" fill="none" width="20" height="20"><circle cx="10" cy="10" r="8" fill="var(--success)"/><path d="M6 10l3 3 5-5" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>
              <svg v-else viewBox="0 0 20 20" fill="none" width="20" height="20"><circle cx="10" cy="10" r="7" stroke="var(--text-3)" stroke-width="1.5"/></svg>
            </div>
            <div class="goal-body">
              <div class="goal-content">{{ g.content }}</div>
              <div class="goal-meta">
                <span class="day-badge" :class="(g.target_day || '').toLowerCase()">{{ chDay(g.target_day) }}</span>
                <span v-if="g.completed && g.completed_at" class="done-time">{{ fmtTime(g.completed_at) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="glass-card">
        <div class="card-header">
          <h3>本月目标 <span class="badge" v-if="monthly.month">{{ monthly.month }}</span></h3>
        </div>
        <div v-if="!monthly.goals?.length" class="empty">暂无月计划</div>
        <div v-else class="goal-list">
          <div v-for="(g, i) in monthly.goals" :key="i" class="goal-item" :class="{ done: g.completed }">
            <div class="goal-check" @click="toggleMonthly(i)">
              <svg v-if="g.completed" viewBox="0 0 20 20" fill="none" width="20" height="20"><circle cx="10" cy="10" r="8" fill="var(--success)"/><path d="M6 10l3 3 5-5" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>
              <svg v-else viewBox="0 0 20 20" fill="none" width="20" height="20"><circle cx="10" cy="10" r="7" stroke="var(--text-3)" stroke-width="1.5"/></svg>
            </div>
            <div class="goal-body">
              <div class="goal-content">{{ g.content }}</div>
              <div class="goal-meta">
                <span v-if="g.completed && g.completed_at" class="done-time">{{ fmtTime(g.completed_at) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="glass-card push-card" v-if="pushes.length">
      <div class="card-header">
        <h3>今日推动 <span class="badge">{{ pushes.length }}</span></h3>
      </div>
      <div class="push-list">
        <div v-for="(p, i) in pushes" :key="i" class="push-item">
          <svg viewBox="0 0 20 20" fill="none" width="16" height="16"><path d="M10 3a7 7 0 017 7v3l2 2H3l2-2v-3a7 7 0 017-7z" stroke="var(--warning)" stroke-width="1.5"/></svg>
          <span>{{ p }}</span>
        </div>
      </div>
    </div>

    <div class="glass-card" v-if="pushHistory.length">
      <div class="card-header">
        <h3>推动历史 <span class="badge">{{ pushHistory.length }} 条</span></h3>
      </div>
      <div class="table-wrap">
        <table>
          <thead><tr><th>时间</th><th>类型</th><th>内容</th></tr></thead>
          <tbody>
            <tr v-for="(h, i) in pushHistory.slice().reverse()" :key="i">
              <td class="mono">{{ fmtTime(h.time) }}</td>
              <td><span class="tag-kind">{{ h.kind }}</span></td>
              <td>{{ h.content }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="glass-card">
      <div class="card-header"><h3>计划系统说明</h3></div>
      <div class="info-list">
        <div class="info-item">📅 每日计划：每天早上自动生成当日任务</div>
        <div class="info-item">📆 每周计划：每周一自动生成周目标，分配到各天</div>
        <div class="info-item">📋 每月计划：每月1号自动生成月目标</div>
        <div class="info-item">🔔 推动系统：每天自动检查计划执行情况并提醒</div>
        <div class="info-item">📂 数据存储于 data/plugin_ai_chat/</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const weekly = ref({ goals: [], week_start: '', total: 0, done: 0 })
const monthly = ref({ goals: [], month: '', total: 0, done: 0 })
const pushes = ref([])
const pushState = ref({})
const pushHistory = ref([])

const weeklyPct = computed(() => weekly.value.total > 0 ? Math.round(weekly.value.done / weekly.value.total * 100) : 0)
const monthlyPct = computed(() => monthly.value.total > 0 ? Math.round(monthly.value.done / monthly.value.total * 100) : 0)

function chDay(en) {
  const map = { Monday: '周一', Tuesday: '周二', Wednesday: '周三', Thursday: '周四', Friday: '周五', Saturday: '周六', Sunday: '周日' }
  return map[en] || en
}
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

async function load() {
  try {
    const d = await api('/api/schedule')
    weekly.value = d.weekly || { goals: [] }
    monthly.value = d.monthly || { goals: [] }
    pushes.value = d.pushes || []
    pushState.value = d.push_state || {}
    pushHistory.value = d.push_history || []
  } catch {}
}

async function toggleWeekly(i) {
  const goal = weekly.value.goals[i]
  if (!goal) return
  goal.completed = !goal.completed
  if (goal.completed) goal.completed_at = Math.floor(Date.now() / 1000)
  else goal.completed_at = 0
  // TODO: sync to backend via schedule API when available
}

async function toggleMonthly(i) {
  const goal = monthly.value.goals[i]
  if (!goal) return
  goal.completed = !goal.completed
  if (goal.completed) goal.completed_at = Math.floor(Date.now() / 1000)
  else goal.completed_at = 0
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.plan-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 16px; margin-bottom: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header { display: flex; align-items: center; gap: 8px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.stat-value { font-size: 22px; font-weight: 700; letter-spacing: -0.3px; }
.stat-label { font-size: 12px; color: var(--text-2); margin-top: 2px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
.bar-wrap { height: 4px; background: var(--surface); border-radius: 2px; overflow: hidden; margin-top: 6px; }
.bar { height: 100%; background: var(--primary); border-radius: 2px; transition: width 0.6s ease; }
.empty { text-align: center; padding: 24px; color: var(--text-3); font-size: 13px; }
.goal-list { display: flex; flex-direction: column; gap: 2px; }
.goal-item { display: flex; align-items: flex-start; gap: 10px; padding: 8px 0; }
.goal-check { cursor: pointer; flex-shrink: 0; margin-top: 1px; }
.goal-body { flex: 1; }
.goal-content { font-size: 13px; line-height: 1.4; }
.goal-item.done .goal-content { color: var(--text-3); text-decoration: line-through; }
.goal-meta { margin-top: 2px; display: flex; gap: 8px; align-items: center; }
.day-badge { font-size: 10px; font-weight: 500; padding: 1px 6px; border-radius: 4px; background: var(--primary-glow); color: var(--primary); }
.done-time { font-size: 10px; color: var(--success); }
.push-card { border-left: 3px solid var(--warning); }
.push-list { display: flex; flex-direction: column; gap: 6px; }
.push-item { display: flex; align-items: center; gap: 8px; font-size: 13px; }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--glass-border); }
td { padding: 6px 12px; border-bottom: 1px solid var(--glass-border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 11px; color: var(--text-2); white-space: nowrap; }
.tag-kind { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--primary-glow); color: var(--primary); }
.info-list { display: flex; flex-direction: column; gap: 8px; }
.info-item { font-size: 13px; color: var(--text-2); }
</style>