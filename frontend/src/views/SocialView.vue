<template>
  <div>
    <div class="card">
      <div class="group-bar">
        <h3>群聊</h3>
        <div class="group-chips" v-if="groups.length">
          <button v-for="g in groups" :key="g" class="group-chip" :class="{ active: g === selected }" @click="select(g)">
            群 {{ g }}
          </button>
        </div>
        <div v-else class="empty-inline">还没有任何群的 social 状态——等群里有消息后这里会出现</div>
      </div>
    </div>

    <div v-if="loading" class="card"><div class="empty">读取中…</div></div>
    <template v-else-if="state">
      <!-- 话题线程 -->
      <div class="card">
        <h3>话题线程 <span class="count-chip" v-if="threads.length">{{ threads.length }}</span></h3>
        <div v-if="threads.length" class="threads">
          <div v-for="t in threads" :key="t.id" class="thread">
            <div class="thread-head">
              <span class="thread-title">「{{ t.title }}」</span>
              <span class="heat" :class="heatClass(t.intensity)">{{ heatLabel(t.intensity) }}</span>
              <span class="thread-time">{{ ago(t.last_active) }}</span>
            </div>
            <div class="intensity-track">
              <div class="intensity-fill" :style="{ width: Math.round(t.intensity * 100) + '%' }"></div>
            </div>
            <div class="thread-members" v-if="memberNames(t).length">
              {{ memberNames(t).join('、') }}
            </div>
            <div class="transcript">
              <div v-for="(l, i) in t.transcript" :key="i" class="tr-line" :class="{ 'tr-bot': l.is_bot }">
                <span class="tr-name">{{ l.is_bot ? '她' : l.name }}</span>
                <span class="tr-text">{{ l.text }}</span>
              </div>
            </div>
            <div class="waiting" v-if="t.unanswered">
              <span class="waiting-icon">⏳</span>
              {{ nameOf(t.unanswered.from) }} 问「{{ t.unanswered.text }}」还没人接
            </div>
          </div>
        </div>
        <div v-else class="empty">此刻没有活跃话题——群里很安静</div>
      </div>

      <div class="grid-2">
        <!-- 参与者注意力 -->
        <div class="card">
          <h3>参与者注意力</h3>
          <div v-if="participants.length" class="attn-list">
            <div v-for="p in participants" :key="p.uid" class="attn-row">
              <span class="attn-name">{{ nameOf(p.uid) }}</span>
              <div class="attn-track">
                <div class="attn-fill" :style="{ width: Math.round(p.attention * 100) + '%' }"></div>
              </div>
              <span class="attn-value">{{ p.attention.toFixed(2) }}</span>
            </div>
          </div>
          <div v-else class="empty">暂无参与者</div>
        </div>

        <!-- 熟络关系 + 等待 -->
        <div class="card">
          <h3>熟络关系</h3>
          <div v-if="bonds.length" class="bond-list">
            <div v-for="(b, i) in bonds" :key="i" class="bond-row">
              <span class="bond-pair">{{ nameOf(b.a) }} ↔ {{ nameOf(b.b) }}</span>
              <span class="bond-level" :class="bondClass(b.v)">{{ bondLabel(b.v) }}</span>
            </div>
          </div>
          <div v-else class="empty">还没有观察到来一往的对话</div>
          <div class="ignored" v-if="ignoredInfo">
            <span class="ignored-icon">🌙</span>
            {{ ignoredInfo }}
          </div>
        </div>
      </div>
    </template>
    <div v-else-if="selected" class="card"><div class="empty">该群暂无社会状态</div></div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'

const groups = ref([])
const selected = ref(null)
const state = ref(null)
const loading = ref(false)
let timer = null

const threads = computed(() =>
  (state.value?.topics || []).slice().sort((a, b) => b.intensity - a.intensity)
)
const participants = computed(() =>
  Object.entries(state.value?.participants || {})
    .map(([uid, p]) => ({ uid: Number(uid), attention: p.attention || 0, last_seen: p.last_seen || 0 }))
    .sort((a, b) => b.attention - a.attention)
    .slice(0, 12)
)
const bonds = computed(() => {
  const out = []
  for (const [a, inner] of Object.entries(state.value?.bonds || {})) {
    for (const [b, v] of Object.entries(inner)) {
      if (v >= 0.25) out.push({ a: Number(a), b: Number(b), v })
    }
  }
  return out.sort((x, y) => y.v - x.v).slice(0, 8)
})
const ignoredInfo = computed(() => {
  const s = state.value
  if (!s || !s.ignored_streak || !s.last_bot_speech) return ''
  return `她上次开口后已连续 ${s.ignored_streak} 条消息没人接`
})

// 名字从转述行里就地解析（后端 participant_name 的前端镜像）
const nameMap = computed(() => {
  const map = {}
  for (const t of state.value?.topics || []) {
    for (const l of t.transcript || []) {
      if (l.speaker && l.name && !map[l.speaker]) map[l.speaker] = l.name
    }
  }
  return map
})
function nameOf(uid) { return nameMap.value[uid] || `用户${uid}` }
function memberNames(t) {
  return (t.participants || []).filter(uid => uid !== 0).map(nameOf)
}

function heatLabel(v) { return v >= 0.6 ? '很热' : v >= 0.3 ? '正聊' : '凉了' }
function heatClass(v) { return v >= 0.6 ? 'hot' : v >= 0.3 ? 'warm' : 'cold' }
function bondLabel(v) { return v >= 0.5 ? '很熟' : '认识' }
function bondClass(v) { return v >= 0.5 ? 'close' : 'known' }

function ago(secs) {
  if (!secs) return ''
  const diff = Math.floor(Date.now() / 1000) - secs
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`
  return `${Math.floor(diff / 86400)} 天前`
}

async function loadGroups() {
  try {
    const j = await api('/api/mind/social')
    groups.value = j.groups || []
    if (!selected.value && groups.value.length) await select(groups.value[0])
    else if (selected.value && !groups.value.includes(selected.value)) {
      selected.value = groups.value[0] || null
      if (selected.value) await select(selected.value)
      else state.value = null
    }
  } catch {}
}

async function select(g) {
  selected.value = g
  loading.value = true
  try {
    const j = await api(`/api/mind/social/${g}`)
    state.value = j.state || null
  } catch { state.value = null }
  loading.value = false
}

onMounted(async () => {
  await loadGroups()
  timer = setInterval(loadGroups, 15000)
})
onUnmounted(() => { if (timer) clearInterval(timer) })
</script>

<style scoped>
.card h3 { font-size: 14px; font-weight: 600; margin-bottom: 10px; }
.group-bar { display: flex; align-items: center; gap: 16px; }
.group-bar h3 { margin-bottom: 0; flex-shrink: 0; }
.group-chips { display: flex; flex-wrap: wrap; gap: 8px; }
.group-chip {
  padding: 5px 14px; border-radius: var(--radius-full);
  border: 1px solid var(--border); background: var(--bg-alt);
  color: var(--text-2); font-size: 12px; font-weight: 500; cursor: pointer;
  transition: var(--transition-fast); font-variant-numeric: tabular-nums;
}
.group-chip:hover { border-color: var(--primary); color: var(--primary); }
.group-chip.active { background: var(--primary); border-color: var(--primary); color: var(--surface-solid); }
.empty-inline { font-size: 13px; color: var(--text-3); }
.empty { font-size: 13px; color: var(--text-3); padding: 8px 0; }
.count-chip {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 18px; height: 18px; padding: 0 6px; margin-left: 6px;
  border-radius: var(--radius-full); background: var(--primary-subtle);
  color: var(--primary); font-size: 11px; font-weight: 600;
}

.threads { display: flex; flex-direction: column; gap: 12px; }
.thread {
  padding: 12px 14px; border-radius: var(--radius-sm);
  border: 1px solid var(--border); background: var(--bg-alt);
}
.thread-head { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.thread-title { font-size: 13px; font-weight: 600; }
.heat { font-size: 11px; font-weight: 600; padding: 1px 8px; border-radius: var(--radius-full); }
.heat.hot { background: var(--danger-subtle, #fde8e4); color: var(--danger, #d65a4a); }
.heat.warm { background: var(--accent-subtle); color: var(--accent); }
.heat.cold { background: var(--border-light); color: var(--text-3); }
.thread-time { font-size: 11px; color: var(--text-3); margin-left: auto; }

.intensity-track {
  height: 4px; border-radius: var(--radius-full);
  background: var(--border-light); margin: 8px 0; overflow: hidden;
}
.intensity-fill { height: 100%; border-radius: var(--radius-full); background: var(--primary); transition: width 0.4s ease; }

.thread-members { font-size: 12px; color: var(--text-2); margin-bottom: 8px; }

.transcript { display: flex; flex-direction: column; gap: 3px; }
.tr-line { font-size: 12px; color: var(--text-2); display: flex; gap: 6px; }
.tr-name { color: var(--text-3); flex-shrink: 0; }
.tr-name::after { content: '：'; }
.tr-bot .tr-name { color: var(--primary); font-weight: 600; }

.waiting {
  margin-top: 8px; padding: 7px 10px; font-size: 12px;
  border-radius: var(--radius-xs); background: var(--accent-subtle);
  color: var(--accent); display: flex; align-items: center; gap: 6px;
}

.grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-top: 16px; }
@media (max-width: 900px) { .grid-2 { grid-template-columns: 1fr; } }

.attn-list { display: flex; flex-direction: column; gap: 8px; }
.attn-row { display: flex; align-items: center; gap: 10px; }
.attn-name { font-size: 12px; color: var(--text-2); width: 72px; flex-shrink: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.attn-track { flex: 1; height: 6px; border-radius: var(--radius-full); background: var(--border-light); overflow: hidden; }
.attn-fill { height: 100%; border-radius: var(--radius-full); background: linear-gradient(90deg, var(--primary), var(--accent)); transition: width 0.4s ease; }
.attn-value { font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; width: 34px; text-align: right; }

.bond-list { display: flex; flex-direction: column; gap: 6px; }
.bond-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.bond-pair { font-size: 12px; color: var(--text-2); }
.bond-level { font-size: 11px; font-weight: 600; padding: 1px 8px; border-radius: var(--radius-full); }
.bond-level.close { background: var(--primary); color: var(--surface-solid); }
.bond-level.known { background: var(--primary-subtle); color: var(--primary); }

.ignored {
  margin-top: 12px; padding: 7px 10px; font-size: 12px;
  border-radius: var(--radius-xs); background: var(--border-light);
  color: var(--text-2); display: flex; align-items: center; gap: 6px;
}
</style>
