<template>
  <div>
    <div class="state-row">
      <div class="state-chip" :class="data?.night ? 'is-night' : 'is-awake'">
        <span class="state-dot"></span>{{ data?.night ? '夜间 · 睡眠中' : '清醒' }}
      </div>
      <div class="state-time" v-if="data?.time">{{ data.time }}</div>
      <div class="state-diary" v-if="data">
        <span class="diary-num">{{ data.diary_today }}</span>
        <span class="diary-label">今日日记</span>
      </div>
    </div>

    <div class="card-row">
      <div class="card half">
        <h3>身体信号</h3>
        <div v-if="bodySignals.length" class="signal-list">
          <div v-for="s in bodySignals" :key="s.name" class="signal-row">
            <span class="signal-name">{{ s.name }}</span>
            <div class="signal-gauge">
              <div class="signal-fill" :style="{ width: gaugePct(s.level), background: gaugeColor(s.level) }"></div>
            </div>
            <span class="signal-level">{{ fmtLevel(s.level) }}</span>
          </div>
        </div>
        <div v-else class="empty">暂无信号</div>
      </div>

      <div class="card half">
        <h3>活跃心事 <span class="count-chip" v-if="loops.length">{{ loops.length }}</span></h3>
        <div v-if="loops.length" class="loop-list">
          <div v-for="l in loops" :key="l.id" class="loop-item">
            <div class="loop-reason">{{ l.reason }}</div>
            <div class="loop-meta">
              <span class="loop-kind">{{ wakeLabel(l.kind) }}</span>
              <span>到期 {{ fmtDue(l.due_at) }}</span>
              <span v-if="l.about_user">关于 {{ l.about_user }}</span>
            </div>
          </div>
        </div>
        <div v-else class="empty">没有惦记的事</div>
      </div>
    </div>

    <div class="card">
      <h3>最近意识流</h3>
      <div v-if="stream.length" class="stream-list">
        <div v-for="(e, i) in stream" :key="i" class="stream-item" :class="'kind-' + e.kind">
          <span class="stream-badge" :class="'badge-' + e.kind">{{ streamLabel(e.kind) }}</span>
          <div class="stream-body">
            <div class="stream-content">{{ e.content }}</div>
            <div class="stream-time">{{ fmtAbs(e.time) }}</div>
          </div>
        </div>
      </div>
      <div v-else class="empty">此刻尚无意识流动</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const data = ref(null)
let timer = null

const POLL_MS = 3000

const bodySignals = computed(() => data.value?.body || [])
const loops = computed(() => data.value?.loops || [])
const stream = computed(() => data.value?.recent_stream || [])

const STREAM_LABELS = {
  sensation: '感官',
  inner: '内心',
  acted: '行动',
  digested: '沉淀',
}
const WAKE_LABELS = {
  idle: '走神',
  digest: '睡前整理',
}

function streamLabel(kind) { return STREAM_LABELS[kind] || kind }
function wakeLabel(kind) { return WAKE_LABELS[kind] || kind }

function gaugePct(level) {
  return Math.max(0, Math.min(1, level)) * 100 + '%'
}
function fmtLevel(level) {
  return Number(level ?? 0).toFixed(2)
}
/* 色温仪表：低偏冷、高偏暖陶土 */
function gaugeColor(level) {
  if (level < 0.34) return 'var(--info)'
  if (level < 0.67) return 'var(--warning)'
  return 'var(--primary)'
}

function fmtDue(secs) {
  const diff = secs - Math.floor(Date.now() / 1000)
  if (diff <= 0) return '随时'
  if (diff < 60) return diff + ' 秒后'
  if (diff < 3600) return Math.floor(diff / 60) + ' 分钟后'
  if (diff < 86400) return Math.floor(diff / 3600) + ' 小时后'
  return Math.floor(diff / 86400) + ' 天后'
}
function fmtAbs(secs) {
  if (!secs) return ''
  return new Date(secs * 1000).toLocaleTimeString('zh-CN', { hour12: false, hour: '2-digit', minute: '2-digit' })
}

async function load() {
  try { data.value = await api('/api/mind/now') } catch {}
}

onMounted(() => {
  load()
  timer = setInterval(load, POLL_MS)
  window.addEventListener('refresh-all', load)
})
onUnmounted(() => {
  clearInterval(timer)
  window.removeEventListener('refresh-all', load)
})
</script>

<style scoped>
.state-row {
  display: flex; align-items: center; gap: 14px; flex-wrap: wrap;
  margin-bottom: 20px;
}
.state-chip {
  display: inline-flex; align-items: center; gap: 8px;
  padding: 6px 14px; border-radius: var(--radius-full);
  font-size: 13px; font-weight: 600;
  border: 1px solid var(--border);
}
.state-chip.is-awake { background: var(--success-subtle); color: var(--success); border-color: var(--success); }
.state-chip.is-night { background: var(--info-subtle); color: var(--info); border-color: var(--info); }
.state-dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
.state-time { font-size: 13px; color: var(--text-2); }
.state-diary {
  display: inline-flex; align-items: baseline; gap: 6px; margin-left: auto;
}
.diary-num { font-size: 20px; font-weight: 700; color: var(--primary); }
.diary-label { font-size: 12px; color: var(--text-2); }

.card-row { display: flex; gap: 16px; margin-bottom: 16px; }
.half { flex: 1; min-width: 0; }
.card h3 { font-size: 14px; font-weight: 600; margin-bottom: 12px; }
.count-chip {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 18px; height: 18px; padding: 0 6px; margin-left: 6px;
  border-radius: var(--radius-full); background: var(--primary-subtle);
  color: var(--primary); font-size: 11px; font-weight: 600;
}

.signal-list { display: flex; flex-direction: column; gap: 10px; }
.signal-row { display: flex; align-items: center; gap: 10px; }
.signal-name { width: 72px; font-size: 12px; color: var(--text-2); flex-shrink: 0; }
.signal-gauge { flex: 1; height: 6px; background: var(--border-light); border-radius: 3px; overflow: hidden; }
.signal-fill { height: 100%; border-radius: 3px; transition: width 0.6s ease, background 0.6s ease; min-width: 2px; }
.signal-level { width: 40px; text-align: right; font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }

.loop-list { display: flex; flex-direction: column; gap: 8px; }
.loop-item {
  padding: 10px 14px; background: var(--bg-alt);
  border: 1px solid var(--border); border-left: 3px solid var(--primary);
  border-radius: var(--radius-xs);
}
.loop-reason { font-size: 13px; line-height: 1.5; }
.loop-meta {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  margin-top: 6px; font-size: 11px; color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.loop-kind {
  padding: 1px 8px; border-radius: var(--radius-full);
  background: var(--warning-subtle); color: var(--warning); font-weight: 500;
}

.stream-list { display: flex; flex-direction: column; gap: 8px; max-height: 480px; overflow-y: auto; }
.stream-item {
  display: flex; gap: 10px; align-items: flex-start;
  padding: 10px 14px; background: var(--bg-alt);
  border: 1px solid var(--border); border-radius: var(--radius-xs);
}
/* 内心：暖棕描边高亮 */
.stream-item.kind-inner {
  border-color: var(--primary);
  box-shadow: inset 0 0 0 1px var(--primary);
  background: var(--primary-subtle);
}
.stream-badge {
  flex-shrink: 0; margin-top: 1px;
  padding: 2px 8px; border-radius: var(--radius-full);
  font-size: 11px; font-weight: 600; line-height: 1.4;
}
.badge-sensation { background: var(--info-subtle); color: var(--info); }
.badge-inner { background: var(--primary); color: var(--surface-solid); }
.badge-acted { background: var(--accent-subtle); color: var(--accent); }
.badge-digested { background: var(--border-light); color: var(--text-2); }
.stream-body { flex: 1; min-width: 0; }
.stream-content { font-size: 13px; line-height: 1.55; white-space: pre-wrap; word-break: break-word; }
.stream-time { margin-top: 4px; font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }

.empty { text-align: center; padding: 28px; color: var(--text-3); font-size: 13px; }
@media (max-width: 768px) { .card-row { flex-direction: column; } }
</style>
