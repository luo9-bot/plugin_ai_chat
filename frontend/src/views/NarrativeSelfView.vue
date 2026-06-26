<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="s in statCards" :key="s.label">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>

    <div class="card-row">
      <div class="card half">
        <div class="card-header">
          <h3>🧠 核心自我认知</h3>
          <button class="btn btn-ghost btn-xs" @click="editIdentity">编辑</button>
        </div>
        <div v-if="data?.core_identity" class="identity-text">{{ data.core_identity }}</div>
        <div v-else class="empty-sm">尚未设置核心自我认知</div>
      </div>
      <div class="card half">
        <div class="card-header">
          <h3>📖 当前叙事线</h3>
          <button class="btn btn-ghost btn-xs" @click="editNarrative">编辑</button>
        </div>
        <div v-if="data?.current_narrative" class="narrative-text">{{ data.current_narrative }}</div>
        <div v-else class="empty-sm">尚未设置当前叙事线</div>
      </div>
    </div>

    <div class="card-row">
      <div class="card half">
        <div class="card-header">
          <h3>💎 内在价值观 <span class="badge">{{ (data?.values || []).length }}</span></h3>
          <button class="btn btn-primary btn-xs" @click="showAddValue = true">＋添加</button>
        </div>
        <div v-if="!(data?.values || []).length" class="empty-sm">暂无价值观记录</div>
        <div v-else class="value-list">
          <div v-for="(v, i) in data.values" :key="i" class="value-item">
            <div class="value-bar">
              <div class="value-fill" :style="{ width: v.strength * 100 + '%' }"></div>
            </div>
            <div class="value-content">
              <span class="value-text">{{ v.content }}</span>
              <span class="value-meta">{{ (v.strength * 100).toFixed(0) }}% · {{ fmtRelTime(v.created_at) }}</span>
            </div>
          </div>
        </div>
      </div>
      <div class="card half">
        <div class="card-header">
          <h3>👁️ 持续关注 <span class="badge">{{ (data?.ongoing_concerns || []).length }}</span></h3>
          <button class="btn btn-primary btn-xs" @click="showAddConcern = true">＋添加</button>
        </div>
        <div v-if="!(data?.ongoing_concerns || []).length" class="empty-sm">暂无持续关注</div>
        <div v-else class="concern-list">
          <div v-for="(c, i) in data.ongoing_concerns" :key="i" class="concern-item">
            <span class="concern-type" :style="{ background: concernColor(c.concern_type) }">{{ concernLabel(c.concern_type) }}</span>
            <span class="concern-text">{{ c.content }}</span>
            <span class="concern-strength">{{ (c.strength * 100).toFixed(0) }}%</span>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-header">
        <h3>📅 时间线 <span class="badge">{{ (data?.timeline || []).length }}</span></h3>
        <button class="btn btn-primary btn-xs" @click="showAddEvent = true">＋添加事件</button>
      </div>
      <div v-if="!(data?.timeline || []).length" class="empty">暂无时间线事件</div>
      <div v-else class="timeline">
        <div v-for="(e, i) in data.timeline" :key="i" class="timeline-item">
          <div class="timeline-dot" :style="{ background: eventColor(e.event_type) }"></div>
          <div class="timeline-card">
            <div class="tl-header">
              <span class="tl-tag" :style="{ background: eventColor(e.event_type) + '22', color: eventColor(e.event_type) }">{{ eventLabel(e.event_type) }}</span>
              <span class="tl-sig" v-if="e.emotional_significance > 0.7">🔥</span>
              <span class="tl-time">{{ fmtRelTime(e.created_at) }}</span>
            </div>
            <div class="tl-content">{{ e.content }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- 添加价值模态框 -->
    <div v-if="showAddValue" class="modal-overlay" @click.self="showAddValue = false">
      <div class="card modal">
        <h3 style="margin-bottom:16px">添加内在价值</h3>
        <label>价值描述</label>
        <input v-model="newValueContent" class="glass-input" placeholder="例如：真诚待人" style="margin-bottom:12px" />
        <label>强度 ({{ (newValueStrength * 100).toFixed(0) }}%)</label>
        <input type="range" v-model.number="newValueStrength" min="0.1" max="1" step="0.1" style="margin-bottom:16px;width:100%" />
        <div class="modal-actions">
          <button class="btn btn-ghost" @click="showAddValue = false">取消</button>
          <button class="btn btn-primary" @click="addValue">保存</button>
        </div>
      </div>
    </div>

    <!-- 添加关注模态框 -->
    <div v-if="showAddConcern" class="modal-overlay" @click.self="showAddConcern = false">
      <div class="card modal">
        <h3 style="margin-bottom:16px">添加持续关注</h3>
        <label>关注内容</label>
        <input v-model="newConcernContent" class="glass-input" placeholder="例如：某人的近况" style="margin-bottom:12px" />
        <label>关注类型</label>
        <select v-model="newConcernType" class="glass-select" style="margin-bottom:12px">
          <option value="Person">人物</option>
          <option value="Event">事件</option>
          <option value="Topic">话题</option>
          <option value="SelfState">自身状态</option>
        </select>
        <div class="modal-actions">
          <button class="btn btn-ghost" @click="showAddConcern = false">取消</button>
          <button class="btn btn-primary" @click="addConcern">保存</button>
        </div>
      </div>
    </div>

    <!-- 添加事件模态框 -->
    <div v-if="showAddEvent" class="modal-overlay" @click.self="showAddEvent = false">
      <div class="card modal">
        <h3 style="margin-bottom:16px">添加时间线事件</h3>
        <label>事件描述</label>
        <input v-model="newEventContent" class="glass-input" placeholder="发生了什么" style="margin-bottom:12px" />
        <label>事件类型</label>
        <select v-model="newEventType" class="glass-select" style="margin-bottom:12px">
          <option value="Conversation">对话</option>
          <option value="Emotional">情感</option>
          <option value="Learning">学习</option>
          <option value="Relationship">关系</option>
          <option value="SelfDiscovery">自我发现</option>
          <option value="Daily">日常</option>
        </select>
        <label>情感重要性 ({{ (newEventSignificance * 100).toFixed(0) }}%)</label>
        <input type="range" v-model.number="newEventSignificance" min="0.1" max="1" step="0.1" style="margin-bottom:16px;width:100%" />
        <div class="modal-actions">
          <button class="btn btn-ghost" @click="showAddEvent = false">取消</button>
          <button class="btn btn-primary" @click="addEvent">保存</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const data = ref(null)
const showAddValue = ref(false)
const showAddConcern = ref(false)
const showAddEvent = ref(false)
const newValueContent = ref('')
const newValueStrength = ref(0.5)
const newConcernContent = ref('')
const newConcernType = ref('Topic')
const newEventContent = ref('')
const newEventType = ref('Daily')
const newEventSignificance = ref(0.5)

const statCards = computed(() => {
  const d = data.value
  const stats = d?.stats || {}
  return [
    { label: '价值观', value: stats.values_count ?? '-', sub: '内在价值', color: '#6366f1' },
    { label: '关注点', value: stats.concerns_count ?? '-', sub: '持续关注', color: '#f59e0b' },
    { label: '时间线', value: stats.timeline_count ?? '-', sub: '事件记录', color: '#34d399' },
  ]
})

function fmtRelTime(ts) {
  if (!ts) return ''
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return '刚刚'
  if (diff < 3600) return Math.floor(diff / 60) + '分钟前'
  if (diff < 86400) return Math.floor(diff / 3600) + '小时前'
  return Math.floor(diff / 86400) + '天前'
}

function concernColor(type) {
  const m = { Person: '#ec4899', Event: '#f59e0b', Topic: '#6366f1', SelfState: '#34d399' }
  return m[type] || '#6b7280'
}
function concernLabel(type) {
  const m = { Person: '人物', Event: '事件', Topic: '话题', SelfState: '自身' }
  return m[type] || type
}
function eventColor(type) {
  const m = { Conversation: '#6366f1', Emotional: '#ec4899', Learning: '#34d399', Relationship: '#f59e0b', SelfDiscovery: '#8b5cf6', Daily: '#6b7280' }
  return m[type] || '#6b7280'
}
function eventLabel(type) {
  const m = { Conversation: '对话', Emotional: '情感', Learning: '学习', Relationship: '关系', SelfDiscovery: '自我发现', Daily: '日常' }
  return m[type] || type
}

async function load() {
  try { data.value = await api('/api/narrative-self') } catch {}
}

async function addValue() {
  if (!newValueContent.value) return
  await api('/api/narrative-self/value', { method: 'POST', body: JSON.stringify({ content: newValueContent.value, strength: newValueStrength.value }) })
  showAddValue.value = false
  newValueContent.value = ''
  load()
}
async function addConcern() {
  if (!newConcernContent.value) return
  await api('/api/narrative-self/concern', { method: 'POST', body: JSON.stringify({ content: newConcernContent.value, type: newConcernType.value, strength: 0.5 }) })
  showAddConcern.value = false
  newConcernContent.value = ''
  load()
}
async function addEvent() {
  if (!newEventContent.value) return
  await api('/api/narrative-self/event', { method: 'POST', body: JSON.stringify({ content: newEventContent.value, event_type: newEventType.value, significance: newEventSignificance.value }) })
  showAddEvent.value = false
  newEventContent.value = ''
  load()
}
async function editIdentity() {
  const val = prompt('输入核心自我认知：', data.value?.core_identity || '')
  if (val !== null && val.trim()) {
    await api('/api/narrative-self/identity', { method: 'POST', body: JSON.stringify({ content: val.trim() }) })
    load()
  }
}
async function editNarrative() {
  const val = prompt('输入当前叙事线：', data.value?.current_narrative || '')
  if (val !== null && val.trim()) {
    await api('/api/narrative-self/narrative', { method: 'POST', body: JSON.stringify({ content: val.trim() }) })
    load()
  }
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 20px; }
.stat-value { font-size: 28px; font-weight: 700; letter-spacing: -0.5px; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; font-weight: 500; }
.stat-sub { font-size: 11px; color: var(--text-3); margin-top: 2px; }
.card-row { display: flex; gap: 16px; margin-bottom: 16px; }
.half { flex: 1; min-width: 0; }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.card-header h3 { font-size: 14px; font-weight: 600; display: flex; align-items: center; gap: 6px; }
.badge { font-size: 10px; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); font-weight: 500; }
.identity-text, .narrative-text { font-size: 14px; line-height: 1.6; color: var(--text); padding: 12px; background: var(--surface-hover); border-radius: var(--radius-sm); }
.empty { text-align: center; padding: 32px; color: var(--text-3); font-size: 13px; }
.empty-sm { text-align: center; padding: 20px; color: var(--text-3); font-size: 12px; }
.value-list { display: flex; flex-direction: column; gap: 10px; }
.value-item { display: flex; align-items: center; gap: 10px; }
.value-bar { width: 60px; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; flex-shrink: 0; }
.value-fill { height: 100%; background: var(--primary); border-radius: 3px; transition: width 0.5s ease; }
.value-content { flex: 1; min-width: 0; }
.value-text { font-size: 13px; display: block; }
.value-meta { font-size: 11px; color: var(--text-3); }
.concern-list { display: flex; flex-direction: column; gap: 8px; }
.concern-item { display: flex; align-items: center; gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--border-light); }
.concern-type { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 4px; color: #fff; flex-shrink: 0; }
.concern-text { flex: 1; font-size: 13px; min-width: 0; }
.concern-strength { font-size: 11px; color: var(--text-3); flex-shrink: 0; }
.timeline { position: relative; padding-left: 24px; }
.timeline::before { content: ''; position: absolute; left: 8px; top: 0; bottom: 0; width: 2px; background: var(--border); }
.timeline-item { position: relative; margin-bottom: 12px; }
.timeline-dot { position: absolute; left: -20px; top: 16px; width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--bg); z-index: 1; }
.timeline-card { padding: 12px 16px; border-radius: var(--radius-sm); background: var(--surface-hover); }
.tl-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
.tl-tag { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 4px; }
.tl-sig { font-size: 12px; }
.tl-time { font-size: 11px; color: var(--text-3); margin-left: auto; }
.tl-content { font-size: 13px; line-height: 1.5; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition-fast); }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); z-index: 200; display: flex; align-items: center; justify-content: center; }
.modal { width: 420px; padding: 24px; }
.modal label { display: block; font-size: 12px; font-weight: 600; margin-bottom: 4px; color: var(--text-2); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
.glass-input, .glass-select { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; width: 100%; }
.glass-input:focus, .glass-select:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-glow); }
@media (max-width: 768px) { .card-row { flex-direction: column; } }
</style>
