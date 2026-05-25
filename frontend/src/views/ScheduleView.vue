<template>
  <div>
    <div class="plan-grid">
      <div class="glass-card">
        <div class="card-header">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><rect x="3" y="4" width="14" height="14" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M3 8h14M7 1v3M13 1v3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
          <h3>本周计划 <span class="badge" v-if="weekly.week_start">{{ weekly.week_start }}</span></h3>
        </div>
        <div v-if="!weekly.goals?.length" class="empty">暂无周计划</div>
        <div v-else class="goal-list">
          <div v-for="(g, i) in weekly.goals" :key="i" class="goal-item" :class="{ done: g.completed }">
            <div class="goal-check" @click="toggleWeekly(i)">
              <svg v-if="g.completed" viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="8" fill="var(--success)"/><path d="M6 10l3 3 5-5" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>
              <svg v-else viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="var(--text-3)" stroke-width="1.5"/></svg>
            </div>
            <div class="goal-body">
              <div class="goal-content">{{ g.content }}</div>
              <div class="goal-meta">
                <span class="day-badge" :class="g.target_day?.toLowerCase()">{{ chDay(g.target_day) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="glass-card">
        <div class="card-header">
          <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M3 5h14v2H3V5z" fill="currentColor" opacity="0.2"/><path d="M4 7h12v11H4V7z" stroke="currentColor" stroke-width="1.5"/><path d="M3 4a1 1 0 011-1h12a1 1 0 011 1v1H3V4z" stroke="currentColor" stroke-width="1.5"/></svg>
          <h3>本月目标 <span class="badge" v-if="monthly.month">{{ monthly.month }}</span></h3>
        </div>
        <div v-if="!monthly.goals?.length" class="empty">暂无月计划</div>
        <div v-else class="goal-list">
          <div v-for="(g, i) in monthly.goals" :key="i" class="goal-item" :class="{ done: g.completed }">
            <div class="goal-check" @click="toggleMonthly(i)">
              <svg v-if="g.completed" viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="8" fill="var(--success)"/><path d="M6 10l3 3 5-5" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>
              <svg v-else viewBox="0 0 20 20" fill="none" width="18" height="18"><circle cx="10" cy="10" r="7" stroke="var(--text-3)" stroke-width="1.5"/></svg>
            </div>
            <div class="goal-content">{{ g.content }}</div>
          </div>
        </div>
      </div>
    </div>

    <div class="glass-card push-card" v-if="pushes.length">
      <div class="card-header">
        <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 3a7 7 0 017 7v3l2 2H3l2-2v-3a7 7 0 017-7z" stroke="currentColor" stroke-width="1.5"/></svg>
        <h3>今日推动</h3>
      </div>
      <div class="push-list">
        <div v-for="(p, i) in pushes" :key="i" class="push-item">
          <svg viewBox="0 0 20 20" fill="none" width="16" height="16"><path d="M10 3a7 7 0 017 7v3l2 2H3l2-2v-3a7 7 0 017-7z" stroke="var(--warning)" stroke-width="1.5"/></svg>
          <span>{{ p }}</span>
        </div>
      </div>
    </div>

    <div class="glass-card">
      <div class="card-header">
        <svg viewBox="0 0 20 20" fill="none" width="18" height="18"><path d="M10 2l7 3v5c0 4-3 7-7 8-4-1-7-4-7-8V5l7-3z" stroke="currentColor" stroke-width="1.5"/></svg>
        <h3>计划系统说明</h3>
      </div>
      <div class="info-list">
        <div class="info-item"><span class="info-icon">📅</span> 每日计划：每天早上自动生成当日任务</div>
        <div class="info-item"><span class="info-icon">📆</span> 每周计划：每周一自动生成周目标，分配到各天</div>
        <div class="info-item"><span class="info-icon">📋</span> 每月计划：每月1号自动生成月目标</div>
        <div class="info-item"><span class="info-icon">🔔</span> 推动系统：每天自动检查计划执行情况并提醒</div>
        <div class="info-item"><span class="info-icon">📂</span> 计划数据存储于 <code>data/plugin_ai_chat/</code></div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const weekly = ref({ goals: [], week_start: '', week_reflection: '' })
const monthly = ref({ goals: [], month: '' })
const pushes = ref([])

function chDay(en) {
  const map = { Monday: '周一', Tuesday: '周二', Wednesday: '周三', Thursday: '周四', Friday: '周五', Saturday: '周六', Sunday: '周日' }
  return map[en] || en
}

async function load() {
  try {
    const d = await api('/api/schedule')
    weekly.value = d.weekly || { goals: [] }
    monthly.value = d.monthly || { goals: [] }
    pushes.value = d.pushes || []
  } catch {}
}

function toggleWeekly(i) {
  const goal = weekly.value.goals[i]
  if (!goal) return
  goal.completed = !goal.completed
  // Auto-save through backend would be nice, but for now just visual
}

function toggleMonthly(i) {
  const goal = monthly.value.goals[i]
  if (!goal) return
  goal.completed = !goal.completed
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.plan-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 16px; margin-bottom: 16px; }
.glass-card {
  padding: 20px; border-radius: var(--radius);
  backdrop-filter: blur(16px) saturate(1.5);
  -webkit-backdrop-filter: blur(16px) saturate(1.5);
  background: var(--surface); border: 1px solid var(--glass-border);
  box-shadow: var(--glass-shadow); margin-bottom: 16px;
}
.card-header { display: flex; align-items: center; gap: 8px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.badge { font-size: 11px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.empty { text-align: center; padding: 32px; color: var(--text-3); font-size: 13px; }
.goal-list { display: flex; flex-direction: column; gap: 2px; }
.goal-item { display: flex; align-items: flex-start; gap: 10px; padding: 8px 0; }
.goal-check { cursor: pointer; flex-shrink: 0; margin-top: 2px; }
.goal-body { flex: 1; }
.goal-content { font-size: 13px; line-height: 1.4; }
.goal-item.done .goal-content { color: var(--text-3); text-decoration: line-through; }
.goal-meta { margin-top: 4px; }
.day-badge { font-size: 10px; font-weight: 500; padding: 1px 6px; border-radius: 4px; background: var(--primary-glow); color: var(--primary); }
.push-card { border-left: 3px solid var(--warning); }
.push-list { display: flex; flex-direction: column; gap: 8px; }
.push-item { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text); }
.info-list { display: flex; flex-direction: column; gap: 10px; }
.info-item { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--text-2); }
.info-icon { font-size: 16px; }
code { font-family: monospace; font-size: 12px; padding: 2px 6px; border-radius: 4px; background: var(--surface); }
</style>