<template>
  <div>
    <div class="card">
      <div class="head-row">
        <h3>审计事件 <span class="count-chip" v-if="events.length">{{ events.length }}</span></h3>
        <div class="summary" v-if="blacklistCount">
          <span class="alert-chip">零容忍拉黑 {{ blacklistCount }}</span>
        </div>
      </div>

      <div v-if="events.length" class="table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>时间</th>
              <th>用户</th>
              <th>滤壳</th>
              <th>处置</th>
              <th>详情</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(e, i) in events" :key="i" :class="{ 'row-blacklist': e.action === 'zero_tolerance_blacklist' }">
              <td class="cell-time">{{ fmtAbs(e.time) }}</td>
              <td class="cell-user">{{ e.user_id ?? '—' }}</td>
              <td><span class="gate-chip">{{ gateLabel(e.gate) }}</span></td>
              <td>
                <span class="action-chip" :class="{ zero: e.action === 'zero_tolerance_blacklist' }">
                  {{ actionLabel(e.action) }}
                </span>
              </td>
              <td class="cell-detail">{{ e.detail }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="empty">滤壳尚未记录任何事件</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const events = ref([])

const GATES = {
  perception: '感知',
  translation: '转译',
  inner_dialog: '内心独白',
  consolidation: '睡前整理',
  persona: '人格',
}
const ACTIONS = {
  zero_tolerance_blacklist: '零容忍拉黑',
  rejected: '拒收',
  quarantined: '隔离',
  warned: '警告',
}

function gateLabel(g) { return GATES[g] || g || '—' }
function actionLabel(a) { return ACTIONS[a] || a || '—' }

const blacklistCount = computed(() => events.value.filter(e => e.action === 'zero_tolerance_blacklist').length)

function fmtAbs(secs) {
  if (!secs) return '—'
  return new Date(secs * 1000).toLocaleString('zh-CN', {
    hour12: false, year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

async function load() {
  try { events.value = (await api('/api/mind/security')).events || [] } catch {}
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.card h3 { font-size: 14px; font-weight: 600; }
.head-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
.count-chip {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 18px; height: 18px; padding: 0 6px; margin-left: 6px;
  border-radius: var(--radius-full); background: var(--primary-subtle);
  color: var(--primary); font-size: 11px; font-weight: 600;
}
.alert-chip {
  padding: 4px 12px; border-radius: var(--radius-full);
  background: var(--accent-subtle); color: var(--accent);
  border: 1px solid var(--accent);
  font-size: 12px; font-weight: 600; font-variant-numeric: tabular-nums;
}

.table-wrap { overflow-x: auto; }
.table td, .table th { vertical-align: top; }
.cell-time { white-space: nowrap; color: var(--text-2); font-size: 12px; font-variant-numeric: tabular-nums; }
.cell-user { white-space: nowrap; font-variant-numeric: tabular-nums; }
.cell-detail { min-width: 200px; word-break: break-word; line-height: 1.5; }

.gate-chip {
  display: inline-block; padding: 2px 8px; border-radius: var(--radius-full);
  background: var(--border-light); color: var(--text-2);
  font-size: 11px; font-weight: 500; white-space: nowrap;
}
.action-chip {
  display: inline-block; padding: 2px 8px; border-radius: var(--radius-full);
  background: var(--warning-subtle); color: var(--warning);
  font-size: 11px; font-weight: 600; white-space: nowrap;
}
/* 零容忍：强调色标红 */
.action-chip.zero { background: var(--accent-subtle); color: var(--accent); }
.row-blacklist td { background: var(--accent-subtle); }
.row-blacklist:hover td { background: var(--accent-subtle); }

.empty { text-align: center; padding: 40px; color: var(--text-3); font-size: 13px; }
</style>
