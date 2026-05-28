<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="s in summaryCards" :key="s.label">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>

    <div class="chart-grid">
      <div class="card chart-card">
        <h3 class="card-title">Token 分布</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="170" height="170">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--surface)" stroke-width="20"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="#6366f1" stroke-width="20"
              :stroke-dasharray="totalArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 1s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="20" font-weight="700">{{ fmtNum(totalTokens) }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="10">总 Tokens</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:#6366f1"></span> Prompt: {{ fmtNum(stats.total_prompt_tokens || 0) }}</div>
            <div class="legend-item"><span class="dot" style="background:#8b5cf6"></span> Completion: {{ fmtNum(stats.total_completion_tokens || 0) }}</div>
            <div class="legend-item"><span class="dot" style="background:#34d399"></span> 缓存命中: {{ stats.cache_hit_ratio || '0%' }}</div>
          </div>
        </div>
      </div>

      <div class="card chart-card">
        <h3 class="card-title">调用分布</h3>
        <div class="chart-container">
          <svg viewBox="0 0 160 160" width="170" height="170">
            <circle cx="80" cy="80" r="64" fill="none" stroke="var(--surface)" stroke-width="20"/>
            <circle cx="80" cy="80" r="64" fill="none" stroke="#f59e0b" stroke-width="20"
              :stroke-dasharray="callArc" stroke-dashoffset="0"
              transform="rotate(-90 80 80)" stroke-linecap="round"
              style="transition: stroke-dasharray 1s ease"/>
            <text x="80" y="74" text-anchor="middle" fill="var(--text)" font-size="20" font-weight="700">{{ stats.total_calls || 0 }}</text>
            <text x="80" y="94" text-anchor="middle" fill="var(--text-2)" font-size="10">总调用</text>
          </svg>
          <div class="chart-legend">
            <div class="legend-item"><span class="dot" style="background:#f59e0b"></span> API 调用次数</div>
            <div class="legend-item"><span class="dot" style="background:#f97316"></span> 缓存命中率</div>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-header">
        <h3>按 Prompt 类型统计</h3>
        <select v-model="sortKey" class="glass-select">
          <option value="total_tokens">按总 Tokens</option>
          <option value="calls">按调用次数</option>
          <option value="avg_total">按平均 Tokens</option>
        </select>
      </div>
      <div v-if="!sortedPrompts.length" class="empty">暂无数据</div>
      <div v-else>
        <div v-for="(p, i) in sortedPrompts" :key="i" class="prompt-row">
          <div class="pr-name">{{ p.name }}</div>
          <div class="pr-bars">
            <div class="pr-stat"><label>调用</label><span class="mono">{{ p.calls }}</span></div>
            <div class="pr-stat"><label>Token</label><span class="mono">{{ fmtNum(p.total_tokens) }}</span></div>
            <div class="pr-stat"><label>平均</label><span class="mono">{{ fmtNum(p.avg_total) }}</span></div>
            <div class="pr-stat"><label>缓存</label><span class="mono">{{ cacheRate(p) }}</span></div>
          </div>
          <div class="pr-bar-wrap">
            <div class="pr-bar bar-pt" :style="{ width: Math.min(p.prompt_tokens / maxToken * 100, 100) + '%' }" title="Prompt Tokens"></div>
            <div class="pr-bar bar-ct" :style="{ width: Math.min(p.completion_tokens / maxToken * 100, 100) + '%' }" title="Completion Tokens"></div>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-header"><h3>最近调用</h3></div>
      <div v-if="!recent.length" class="empty">暂无记录</div>
      <div v-else class="table-wrap">
        <table>
          <thead><tr><th>时间</th><th>Prompt</th><th>模型</th><th>Prompt Tokens</th><th>Completion</th><th>总 Tokens</th><th>缓存命中</th><th>缓存未中</th></tr></thead>
          <tbody>
            <tr v-for="(r, i) in recent" :key="i">
              <td class="mono">{{ fmtTime(r.time) }}</td>
              <td><span class="tag">{{ r.prompt }}</span></td>
              <td class="mono" style="font-size:11px">{{ r.model?.split('/')?.pop() || r.model }}</td>
              <td class="mono">{{ r.prompt_tokens }}</td>
              <td class="mono">{{ r.completion_tokens }}</td>
              <td class="mono" style="font-weight:600">{{ r.total_tokens }}</td>
              <td class="mono" style="color:var(--success)">{{ r.cache_hit || '-' }}</td>
              <td class="mono" style="color:var(--danger)">{{ r.cache_miss || '-' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'

const stats = ref({})
const sortKey = ref('total_tokens')

function fmtNum(n) { if (n == null) return '-'; if (n >= 1000000) return (n/1000000).toFixed(1) + 'M'; if (n >= 1000) return (n/1000).toFixed(1) + 'K'; return String(n) }
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
function cacheRate(p) { if (p.cache_hit > 0) { var total = p.cache_hit + p.cache_miss; return (p.cache_hit / Math.max(total, 1) * 100).toFixed(1) + '%' } return '-' }

const totalTokens = computed(() => (stats.value.total_tokens || 0))
const maxToken = computed(() => { const m = Math.max(...sortedPrompts.value.map(p => p.total_tokens), 1); return m })

const summaryCards = computed(() => [
  { label: '总调用次数', value: stats.value.total_calls ?? '-', sub: 'API 请求', color: '#6366f1' },
  { label: '总 Tokens', value: fmtNum(stats.value.total_tokens), sub: 'Prompt + Completion', color: '#8b5cf6' },
  { label: '平均每次', value: stats.value.total_calls > 0 ? fmtNum(Math.round(stats.value.total_tokens / stats.value.total_calls)) : '-', sub: 'Tokens/调用', color: '#34d399' },
  { label: '缓存命中率', value: stats.value.cache_hit_ratio || '0%', sub: '节省 Tokens', color: '#f59e0b' },
])

const sortedPrompts = computed(() => {
  const list = stats.value.by_prompt || []
  const key = sortKey.value
  return [...list].sort((a, b) => (b[key] || 0) - (a[key] || 0))
})

const recent = computed(() => stats.value.recent || [])

const totalArc = computed(() => {
  const total = totalTokens.value
  const pct = Math.min(total / 10000000, 1)
  const circ = 2 * Math.PI * 64
  return `${circ * pct} ${circ * (1 - pct)}`
})

const callArc = computed(() => {
  const total = stats.value.total_calls || 0
  const pct = Math.min(total / 5000, 1)
  const circ = 2 * Math.PI * 64
  return `${circ * pct} ${circ * (1 - pct)}`
})

async function load() {
  try {
    const d = await (await fetch('/api/analytics', { headers: { 'Authorization': 'Bearer ' + localStorage.getItem('admin_token') } })).json()
    stats.value = d
  } catch {}
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 16px; margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.glass-select { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.stat-value { font-size: 26px; font-weight: 700; letter-spacing: -0.5px; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
.chart-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; margin-bottom: 16px; }
.chart-card { text-align: center; }
.card-title { font-size: 14px; font-weight: 600; margin-bottom: 16px; text-align: left; }
.chart-container { display: flex; align-items: center; justify-content: center; gap: 20px; flex-wrap: wrap; }
.chart-legend { display: flex; flex-direction: column; gap: 8px; text-align: left; }
.legend-item { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-2); }
.dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.prompt-row { padding: 10px 0; border-bottom: 1px solid var(--border); }
.prompt-row:last-child { border-bottom: none; }
.pr-name { font-size: 13px; font-weight: 600; margin-bottom: 4px; text-transform: capitalize; }
.pr-bars { display: flex; gap: 16px; font-size: 12px; margin-bottom: 6px; }
.pr-stat { display: flex; gap: 4px; align-items: center; }
.pr-stat label { color: var(--text-3); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.pr-bar-wrap { height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; display: flex; gap: 2px; }
.pr-bar { height: 100%; border-radius: 3px; transition: width 0.5s ease; }
.bar-pt { background: #6366f1; }
.bar-ct { background: #8b5cf6; }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 12px; }
th { text-align: left; padding: 8px 10px; font-weight: 600; font-size: 10px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--border); }
td { padding: 6px 10px; border-bottom: 1px solid var(--border); }
tr:hover td { background: var(--surface-hover); }
.tag { font-size: 10px; padding: 2px 6px; border-radius: 4px; background: var(--primary-glow); color: var(--primary); }
</style>