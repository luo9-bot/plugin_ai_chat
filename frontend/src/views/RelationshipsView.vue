<template>
  <div>
    <div class="card">
      <div class="card-header">
        <h3>关系管理</h3>
        <div class="header-actions">
          <input v-model="search" placeholder="搜索用户ID..." class="glass-input" style="width:150px" />
          <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
        </div>
      </div>
      <div v-if="!Object.keys(relationships).length" class="empty">暂无关系数据</div>
      <div v-else class="rel-grid">
        <div v-for="(rel, uid) in filteredRels" :key="uid" class="rel-card" @click="selectUser(uid)">
          <div class="rel-header">
            <span class="rel-uid">用户 {{ uid }}</span>
            <span class="rel-type" :style="{ background: typeColor(rel.relationship_type) }">{{ typeLabel(rel.relationship_type) }}</span>
          </div>
          <div class="rel-bars">
            <div class="rel-bar-row" v-for="bar in getBars(rel)" :key="bar.label">
              <span class="rel-bar-label">{{ bar.label }}</span>
              <div class="rel-bar-wrap"><div class="rel-bar" :style="{ width: bar.value * 100 + '%', background: bar.color }"></div></div>
              <span class="rel-bar-pct">{{ (bar.value * 100).toFixed(0) }}%</span>
            </div>
          </div>
          <div class="rel-footer">
            <span>互动 {{ rel.interaction_count || 0 }}次</span>
            <span>{{ fmtRelTime(rel.last_interaction) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 关系详情模态框 -->
    <div v-if="selectedDetail" class="modal-overlay" @click.self="selectedDetail = null">
      <div class="card modal modal-lg">
        <div class="modal-header">
          <h3>用户 {{ selectedUid }} 的关系详情</h3>
          <button class="btn btn-ghost btn-xs" @click="selectedDetail = null">✕</button>
        </div>
        <div class="detail-grid">
          <div class="detail-section">
            <h4>关系维度</h4>
            <div class="kv-grid">
              <div class="kv"><span>信任</span><strong>{{ (selectedDetail.trust * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>亲密</span><strong>{{ (selectedDetail.intimacy * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>默契</span><strong>{{ (selectedDetail.rapport * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>好感</span><strong>{{ (selectedDetail.affection * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>互惠</span><strong>{{ (selectedDetail.reciprocity * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>紧张</span><strong :style="{ color: selectedDetail.tension > 0.5 ? 'var(--danger)' : 'inherit' }">{{ (selectedDetail.tension * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>烦躁</span><strong :style="{ color: selectedDetail.annoyance > 0.5 ? 'var(--warning)' : 'inherit' }">{{ (selectedDetail.annoyance * 100).toFixed(0) }}%</strong></div>
              <div class="kv"><span>好奇</span><strong>{{ (selectedDetail.curiosity * 100).toFixed(0) }}%</strong></div>
            </div>
          </div>
          <div class="detail-section">
            <h4>互动统计</h4>
            <div class="kv-grid">
              <div class="kv"><span>关系类型</span><strong>{{ typeLabel(selectedDetail.relationship_type) }}</strong></div>
              <div class="kv"><span>总互动</span><strong>{{ selectedDetail.interaction_count }}</strong></div>
              <div class="kv"><span>积极互动</span><strong style="color:var(--success)">{{ selectedDetail.positive_interactions }}</strong></div>
              <div class="kv"><span>消极互动</span><strong style="color:var(--danger)">{{ selectedDetail.negative_interactions }}</strong></div>
              <div class="kv"><span>连续忽视</span><strong :style="{ color: selectedDetail.ignore_streak > 2 ? 'var(--warning)' : 'inherit' }">{{ selectedDetail.ignore_streak }}次</strong></div>
              <div class="kv"><span>共享记忆</span><strong>{{ selectedDetail.shared_memories_count }}</strong></div>
            </div>
          </div>
        </div>
        <div v-if="selectedDetail.impression" class="detail-section" style="margin-top:12px">
          <h4>印象</h4>
          <div class="impression-text">{{ selectedDetail.impression }}</div>
        </div>
        <div v-if="(selectedDetail.recent_events || []).length" class="detail-section" style="margin-top:12px">
          <h4>最近事件</h4>
          <div class="event-list">
            <div v-for="(e, i) in selectedDetail.recent_events" :key="i" class="event-item">
              <span class="event-type">{{ e.event }}</span>
              <span class="event-detail" v-if="e.detail">{{ e.detail }}</span>
              <span class="event-time">{{ fmtRelTime(e.timestamp) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const relationships = ref({})
const search = ref('')
const selectedDetail = ref(null)
const selectedUid = ref(null)

const filteredRels = computed(() => {
  if (!search.value) return relationships.value
  const q = search.value
  const result = {}
  for (const [uid, rel] of Object.entries(relationships.value)) {
    if (uid.includes(q)) result[uid] = rel
  }
  return result
})

function getBars(rel) {
  return [
    { label: '信任', value: rel.trust || 0, color: '#6366f1' },
    { label: '亲密', value: rel.intimacy || 0, color: '#ec4899' },
    { label: '好感', value: rel.affection || 0, color: '#f59e0b' },
    { label: '互惠', value: rel.reciprocity || 0.5, color: '#34d399' },
  ]
}

function typeColor(type) {
  const m = { stranger: '#6b7280', acquaintance: '#60a5fa', regular: '#34d399', close: '#ec4899', confidant: '#f59e0b', antagonistic: '#ef4444', admiring: '#f472b6' }
  return m[type] || '#6b7280'
}
function typeLabel(type) {
  const m = { stranger: '陌生人', acquaintance: '认识', regular: '常客', close: '亲近', confidant: '知己', antagonistic: '对立', admiring: '仰慕' }
  return m[type] || type
}

function fmtRelTime(ts) {
  if (!ts) return ''
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return '刚刚'
  if (diff < 3600) return Math.floor(diff / 60) + '分钟前'
  if (diff < 86400) return Math.floor(diff / 3600) + '小时前'
  return Math.floor(diff / 86400) + '天前'
}

async function selectUser(uid) {
  try {
    selectedUid.value = uid
    selectedDetail.value = await api(`/api/relationships/${uid}`)
  } catch {
    selectedDetail.value = null
  }
}

async function load() {
  try {
    const d = await api('/api/relationships')
    relationships.value = d.relationships || {}
  } catch {}
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 6px; align-items: center; }
.glass-input { padding: 6px 10px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 12px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
.rel-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
.rel-card { padding: 14px; border-radius: var(--radius-sm); background: var(--surface-hover); cursor: pointer; transition: var(--transition); }
.rel-card:hover { box-shadow: var(--glass-shadow-lg); transform: translateY(-1px); }
.rel-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
.rel-uid { font-size: 13px; font-weight: 600; font-family: monospace; }
.rel-type { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 4px; color: #fff; }
.rel-bars { display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px; }
.rel-bar-row { display: flex; align-items: center; gap: 6px; }
.rel-bar-label { width: 28px; font-size: 10px; color: var(--text-3); flex-shrink: 0; }
.rel-bar-wrap { flex: 1; height: 4px; background: var(--surface); border-radius: 2px; overflow: hidden; }
.rel-bar { height: 100%; border-radius: 2px; transition: width 0.5s ease; }
.rel-bar-pct { width: 30px; font-size: 10px; color: var(--text-3); text-align: right; flex-shrink: 0; }
.rel-footer { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-3); }
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); z-index: 200; display: flex; align-items: center; justify-content: center; }
.modal { width: 480px; max-height: 80vh; overflow-y: auto; padding: 24px; }
.modal-lg { width: 600px; }
.modal-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.modal-header h3 { font-size: 16px; font-weight: 600; }
.detail-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
.detail-section h4 { font-size: 12px; font-weight: 600; color: var(--text-2); margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.5px; }
.kv-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
.kv { display: flex; justify-content: space-between; align-items: center; font-size: 13px; padding: 4px 0; border-bottom: 1px solid var(--border-light); }
.kv span { color: var(--text-2); }
.kv strong { font-weight: 600; }
.impression-text { font-size: 13px; line-height: 1.5; padding: 10px; background: var(--surface-hover); border-radius: var(--radius-xs); }
.event-list { display: flex; flex-direction: column; gap: 6px; }
.event-item { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.event-type { font-weight: 600; color: var(--primary); }
.event-detail { flex: 1; color: var(--text-2); }
.event-time { color: var(--text-3); font-size: 11px; }
@media (max-width: 768px) { .detail-grid { grid-template-columns: 1fr; } }
</style>
