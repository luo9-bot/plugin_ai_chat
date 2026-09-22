<template>
  <div>
    <div class="card">
      <div class="date-bar">
        <h3>日期</h3>
        <div class="date-chips" v-if="dates.length">
          <button v-for="d in dates" :key="d" class="date-chip" :class="{ active: d === selected }" @click="select(d)">
            {{ d }}
          </button>
        </div>
        <div v-else class="empty-inline">暂无记录</div>
      </div>
    </div>

    <div class="card">
      <div class="filter-bar">
        <h3>事件 <span class="count-chip" v-if="filtered.length">{{ filtered.length }}</span></h3>
        <div class="kind-filters">
          <button class="kind-btn" :class="{ active: filter === 'all' }" @click="filter = 'all'">全部</button>
          <button v-for="k in KINDS" :key="k.value" class="kind-btn" :class="{ active: filter === k.value }" @click="filter = k.value">
            {{ k.label }}
          </button>
        </div>
      </div>

      <div v-if="loading" class="empty">读取中…</div>
      <div v-else-if="filtered.length" class="timeline">
        <div v-for="(e, i) in filtered" :key="i" class="tl-item" :class="['kind-' + e.kind, { 'is-recall': e.recall }]">
          <div class="tl-rail">
            <span class="tl-dot"></span>
            <span class="tl-line" v-if="i < filtered.length - 1"></span>
          </div>
          <div class="tl-body">
            <div class="tl-head">
              <span class="tl-badge" :class="'badge-' + displayKind(e)">{{ kindLabel(displayKind(e)) }}</span>
              <span class="tl-time">{{ fmtAbs(e.time) }}</span>
              <span class="tl-about" v-if="e.about">关于 {{ e.about }}</span>
              <span class="recall-source" v-if="e.recall">{{ e.recall.source }}</span>
              <span class="recall-status" v-if="e.recall">已自动处理</span>
            </div>
            <div class="tl-content">{{ e.content }}</div>
          </div>
        </div>
      </div>
      <div v-else class="empty">这一天她没有留下事件</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const dates = ref([])
const selected = ref('')
const events = ref([])
const filter = ref('all')
const loading = ref(false)

const KINDS = [
  { value: 'sensation', label: '感官' },
  { value: 'inner', label: '内心' },
  { value: 'acted', label: '行动' },
  { value: 'digested', label: '沉淀' },
  { value: 'recall', label: '联想' },
]
const LABELS = Object.fromEntries(KINDS.map(k => [k.value, k.label]))

function kindLabel(kind) { return LABELS[kind] || kind }

function isRecall(e) {
  return Boolean(e.recall) || String(e.content || '').startsWith('想起：') || String(e.content || '').startsWith('（毫无来由地')
}
function displayKind(e) { return isRecall(e) ? 'recall' : e.kind }
const filtered = computed(() => {
  if (filter.value === 'all') return events.value
  if (filter.value === 'recall') return events.value.filter(isRecall)
  return events.value.filter(e => e.kind === filter.value && !isRecall(e))
})

function fmtAbs(secs) {
  if (!secs) return ''
  return new Date(secs * 1000).toLocaleString('zh-CN', {
    hour12: false, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  })
}

async function loadDates() {
  try {
    const j = await api('/api/mind/stream')
    dates.value = (j.dates || []).slice().sort().reverse()
    if (!selected.value && dates.value.length) await select(dates.value[0])
  } catch {}
}

async function loadDate(date) {
  loading.value = true
  try {
    const j = await api(`/api/mind/stream/${encodeURIComponent(date)}`)
    events.value = (j.events || []).slice().sort((a, b) => b.time - a.time)
  } catch { events.value = [] }
  loading.value = false
}

async function select(date) {
  selected.value = date
  filter.value = 'all'
  await loadDate(date)
}

let poller
onMounted(async () => {
  await loadDates()
  poller = window.setInterval(() => {
    if (selected.value) loadDate(selected.value)
  }, 5000)
})
onUnmounted(() => window.clearInterval(poller))
</script>

<style scoped>
.card h3 { font-size: 14px; font-weight: 600; margin-bottom: 10px; }
.date-bar { display: flex; align-items: center; gap: 16px; }
.date-bar h3 { margin-bottom: 0; flex-shrink: 0; }
.date-chips { display: flex; flex-wrap: wrap; gap: 8px; }
.date-chip {
  padding: 5px 14px; border-radius: var(--radius-full);
  border: 1px solid var(--border); background: var(--bg-alt);
  color: var(--text-2); font-size: 12px; font-weight: 500; cursor: pointer;
  transition: var(--transition-fast); font-variant-numeric: tabular-nums;
}
.date-chip:hover { border-color: var(--primary); color: var(--primary); }
.date-chip.active { background: var(--primary); border-color: var(--primary); color: var(--surface-solid); }
.empty-inline { font-size: 13px; color: var(--text-3); }

.filter-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.count-chip {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 18px; height: 18px; padding: 0 6px; margin-left: 6px;
  border-radius: var(--radius-full); background: var(--primary-subtle);
  color: var(--primary); font-size: 11px; font-weight: 600;
}
.kind-filters { display: flex; flex-wrap: wrap; gap: 6px; }
.kind-btn {
  padding: 4px 12px; border-radius: var(--radius-full);
  border: 1px solid var(--border); background: transparent;
  color: var(--text-2); font-size: 12px; font-weight: 500; cursor: pointer;
  transition: var(--transition-fast);
}
.kind-btn:hover { border-color: var(--primary); color: var(--primary); }
.kind-btn.active { background: var(--primary-subtle); border-color: var(--primary); color: var(--primary); }

.timeline { display: flex; flex-direction: column; margin-top: 6px; }
.tl-item { display: flex; gap: 12px; }
.tl-rail { display: flex; flex-direction: column; align-items: center; width: 14px; flex-shrink: 0; padding-top: 16px; }
.tl-dot { width: 9px; height: 9px; border-radius: 50%; background: var(--border); flex-shrink: 0; }
.tl-item.kind-inner .tl-dot { background: var(--primary); }
.tl-item.kind-acted .tl-dot { background: var(--accent); }
.tl-item.kind-sensation .tl-dot { background: var(--info); }
.tl-item.is-recall .tl-dot { background: var(--warning); }
.tl-line { flex: 1; width: 2px; background: var(--border-light); min-height: 14px; }
.tl-body { flex: 1; min-width: 0; padding-bottom: 16px; }
.tl-head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.tl-badge {
  padding: 2px 8px; border-radius: var(--radius-full);
  font-size: 11px; font-weight: 600; line-height: 1.4;
}
.badge-sensation { background: var(--info-subtle); color: var(--info); }
.badge-inner { background: var(--primary); color: var(--surface-solid); }
.badge-acted { background: var(--accent-subtle); color: var(--accent); }
.badge-digested { background: var(--border-light); color: var(--text-2); }
.tl-time { font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }
.tl-about { font-size: 11px; color: var(--text-3); }
.recall-source, .recall-status { font-size: 10px; padding: 2px 6px; border-radius: 4px; background: var(--warning-subtle); color: var(--warning); }
.recall-status { background: var(--success-subtle); color: var(--success); }
.tl-content {
  margin-top: 5px; font-size: 13px; line-height: 1.55;
  white-space: pre-wrap; word-break: break-word;
}
.tl-item.kind-inner .tl-content { color: var(--text); }
.tl-item.is-recall .tl-content { padding: 8px 10px; border-left: 3px solid var(--warning); background: var(--warning-subtle); border-radius: 4px; }

.empty { text-align: center; padding: 32px; color: var(--text-3); font-size: 13px; }
</style>
