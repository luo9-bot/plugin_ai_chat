<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="tf in ORDER" :key="tf">
        <div class="stat-value">{{ stat(tf).done }}/{{ stat(tf).total }}</div>
        <div class="stat-label">{{ stat(tf).label }}完成</div>
        <div class="bar-wrap"><div class="bar" :style="{ width: pct(tf) + '%' }"></div></div>
      </div>
      <div class="card">
        <div class="stat-value">{{ history.length }}</div>
        <div class="stat-label">状态变更记录</div>
        <div class="stat-sub">运行时计划状态</div>
      </div>
    </div>

    <div class="plan-grid">
      <div class="card" v-for="tf in ORDER" :key="tf">
        <div class="card-header">
          <h3>{{ data[tf].label }}的事 <span class="badge" v-if="data[tf].period">{{ data[tf].period }}</span></h3>
        </div>
        <div v-if="!data[tf].items.length" class="empty">还没有计划</div>
        <div v-else class="goal-list">
          <div v-for="g in data[tf].items" :key="g.id" class="goal-item" :class="{ done: g.completed }">
            <div class="goal-check" :title="g.completed ? '已自动完成' : '进行中'">
              <svg v-if="g.completed" viewBox="0 0 20 20" fill="none" width="20" height="20"><circle cx="10" cy="10" r="8" fill="var(--success)"/><path d="M6 10l3 3 5-5" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg>
              <svg v-else viewBox="0 0 20 20" fill="none" width="20" height="20"><circle cx="10" cy="10" r="7" stroke="var(--text-3)" stroke-width="1.5"/></svg>
            </div>
            <div class="goal-body">
              <div class="goal-content"><span class="goal-id">{{ g.id }}</span>{{ g.content }}</div>
              <div class="goal-meta">
                <span v-if="g.target_day" class="day-badge" :class="(g.target_day || '').toLowerCase()">{{ chDay(g.target_day) }}</span>
                <span v-if="g.completion_note" class="done-note">{{ g.completion_note }}</span>
                <span v-if="g.progress && g.progress.length" class="progress-note">{{ g.progress[g.progress.length - 1] }}</span>
                <span v-if="g.completed && g.completed_at" class="done-time">{{ fmtTime(g.completed_at) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="card" v-if="history.length">
      <div class="card-header">
        <h3>状态变更历史 <span class="badge">{{ history.length }} 条</span></h3>
      </div>
      <div class="table-wrap">
        <table>
          <thead><tr><th>时间</th><th>类型</th><th>内容</th></tr></thead>
          <tbody>
            <tr v-for="(h, i) in history.slice().reverse()" :key="i">
              <td class="mono">{{ fmtTime(h.time) }}</td>
              <td><span class="tag-kind">{{ h.kind }}</span></td>
              <td>{{ h.content }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="card">
      <div class="card-header"><h3>计划系统说明</h3></div>
      <div class="info-list">
        <div class="info-item">日/周/月共用一套模型：每条计划有稳定编号（d/w/m 前缀），她通过编号指认要动哪一条。</div>
        <div class="info-item">每日计划：跨天时自动生成当日的 2-4 件事。</div>
        <div class="info-item">每周计划：跨周时生成周目标并分配到具体某天。</div>
        <div class="info-item">每月计划：跨月时生成本月目标。</div>
        <div class="info-item">完成判定由运行时自动记录：她通过 finish_plan 完成、用 note_progress 记进展、用 add_plan 临时加事。</div>
        <div class="info-item">本页只读展示每日、每周、每月计划，定时从后端重新读取，不维护自己的状态副本。</div>
        <div class="info-item">数据存储于 data/plugin_ai_chat/</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

/** 展示顺序：越近的跨度越靠前 */
const ORDER = ['day', 'week', 'month']

function emptyFrame(label) {
  return { label, period: '', items: [], total: 0, done: 0 }
}

const data = ref({
  day: emptyFrame('今日'),
  week: emptyFrame('本周'),
  month: emptyFrame('本月'),
})
const history = ref([])

function stat(tf) {
  return data.value[tf] || emptyFrame('')
}
function pct(tf) {
  const s = stat(tf)
  return s.total > 0 ? Math.round((s.done / s.total) * 100) : 0
}

function chDay(en) {
  const map = { Monday: '周一', Tuesday: '周二', Wednesday: '周三', Thursday: '周四', Friday: '周五', Saturday: '周六', Sunday: '周日' }
  return map[en] || en
}
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

async function load() {
  try {
    const d = await api('/api/schedule')
    const frames = d.timeframes || {}
    for (const tf of ORDER) {
      data.value[tf] = frames[tf] || emptyFrame('')
    }
    history.value = d.history || []
  } catch {}
}

let poller
onMounted(() => {
  load()
  poller = window.setInterval(load, 5000)
  window.addEventListener('refresh-all', load)
})
onUnmounted(() => {
  window.clearInterval(poller)
  window.removeEventListener('refresh-all', load)
})
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.plan-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 16px; margin-bottom: 16px; }
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
.goal-check { flex-shrink: 0; margin-top: 1px; }
.goal-body { flex: 1; }
.goal-content { font-size: 13px; line-height: 1.4; }
.goal-id { font-family: monospace; font-size: 10px; color: var(--text-3); margin-right: 6px; padding: 1px 4px; border-radius: 3px; background: var(--surface); }
.goal-item.done .goal-content { color: var(--text-3); text-decoration: line-through; }
.goal-meta { margin-top: 2px; display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.day-badge { font-size: 10px; font-weight: 500; padding: 1px 6px; border-radius: 4px; background: var(--primary-glow); color: var(--primary); }
.done-time { font-size: 10px; color: var(--success); }
.done-note { font-size: 10px; color: var(--success); }
.progress-note { font-size: 10px; color: var(--text-3); }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--border); }
td { padding: 6px 12px; border-bottom: 1px solid var(--border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 11px; color: var(--text-2); white-space: nowrap; }
.tag-kind { font-size: 10px; padding: 1px 6px; border-radius: 3px; background: var(--primary-glow); color: var(--primary); }
.info-list { display: flex; flex-direction: column; gap: 8px; }
.info-item { font-size: 13px; color: var(--text-2); }
</style>
