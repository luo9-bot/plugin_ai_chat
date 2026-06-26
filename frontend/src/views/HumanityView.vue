<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="s in stats" :key="s.label">
        <div class="stat-icon" :style="{ color: s.color }" v-html="s.icon"></div>
        <div class="stat-value">{{ s.value ?? '-' }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>

    <div class="section-title">功能开关</div>
    <div class="toggle-grid">
      <div v-for="(v, k) in data?.config_enabled || {}" :key="k" class="toggle-chip" :class="{ on: v }">
        {{ labelMap[k] || k }}
      </div>
    </div>

    <div class="card-row">
      <div class="card half" v-if="data?.social_battery">
        <h3>社交电量</h3>
        <div class="battery-visual">
          <div class="battery-fill" :style="{ width: data.social_battery.percentage * 100 + '%', background: batteryColor }"></div>
        </div>
        <div class="kv-grid">
          <div class="kv"><span>电量</span><strong>{{ data.social_battery.level.toFixed(1) }} / {{ data.social_battery.capacity }}</strong></div>
          <div class="kv"><span>百分比</span><strong>{{ (data.social_battery.percentage * 100).toFixed(1) }}%</strong></div>
          <div class="kv"><span>倦怠</span><strong :style="{ color: data.social_battery.is_burned_out ? 'var(--danger)' : 'var(--success)' }">{{ data.social_battery.is_burned_out ? '是' : '否' }}</strong></div>
          <div class="kv"><span>模式</span><strong>{{ data.social_battery.is_passive_mode ? '被动' : '主动' }}</strong></div>
          <div class="kv"><span>活跃分钟</span><strong>{{ data.social_battery.active_minutes }}</strong></div>
        </div>
      </div>

      <div class="card half" v-if="data?.circadian">
        <h3>昼夜节律</h3>
        <div class="rhythm-bars">
          <div v-for="r in rhythmBars" :key="r.label" class="rhythm-row">
            <span class="rhythm-label">{{ r.label }}</span>
            <div class="rhythm-bar-wrap"><div class="rhythm-bar" :style="{ width: r.value * 100 + '%', background: r.color }"></div></div>
            <span class="rhythm-pct">{{ (r.value * 100).toFixed(0) }}%</span>
          </div>
        </div>
        <div class="kv-grid" style="margin-top:12px">
          <div class="kv"><span>当前时间</span><strong>{{ data.circadian.current_hour.toFixed(1) }}时</strong></div>
          <div class="kv"><span>免打扰</span><strong :style="{ color: data.circadian.is_quiet_hours ? 'var(--warning)' : 'var(--success)' }">{{ data.circadian.is_quiet_hours ? '是' : '否' }}</strong></div>
        </div>
      </div>
    </div>

    <div class="card-row">
      <div class="card half" v-if="data?.attention">
        <h3>注意力</h3>
        <div class="kv-grid">
          <div class="kv"><span>注意力水平</span><strong>{{ (data.attention.attention_level * 100).toFixed(0) }}%</strong></div>
          <div class="kv"><span>心流状态</span><strong>{{ (data.attention.flow_state * 100).toFixed(0) }}%</strong></div>
          <div class="kv"><span>专注话题</span><strong>{{ data.attention.focused_topic || '—' }}</strong></div>
          <div class="kv"><span>心流恢复中</span><strong :style="{ color: data.attention.flow_recovering ? 'var(--warning)' : 'var(--success)' }">{{ data.attention.flow_recovering ? '是' : '否' }}</strong></div>
        </div>
      </div>

      <div class="card half" v-if="data?.cognitive_biases">
        <h3>认知偏差</h3>
        <div class="kv-grid">
          <div class="kv"><span>确认偏误</span><strong>{{ data.cognitive_biases.confirmation_bias.toFixed(2) }}</strong></div>
          <div class="kv"><span>近因效应</span><strong>{{ data.cognitive_biases.recency_bias.toFixed(2) }}</strong></div>
          <div class="kv"><span>情绪一致性</span><strong>{{ data.cognitive_biases.mood_congruence.toFixed(2) }}</strong></div>
          <div class="kv"><span>锚定效应</span><strong>{{ data.cognitive_biases.anchoring_strength.toFixed(2) }}</strong></div>
          <div class="kv"><span>可得性启发</span><strong>{{ data.cognitive_biases.availability_heuristic.toFixed(2) }}</strong></div>
        </div>
      </div>
    </div>

    <div class="card-row">
      <div class="card half" v-if="data?.motivation">
        <h3>主动消息动机</h3>
        <div class="motivation-badge">
          当前最强动机: <strong>{{ data.motivation.type }}</strong> (强度: {{ data.motivation.strength.toFixed(2) }})
        </div>
      </div>
      <div class="card half" v-if="data?.narrative_self?.core_identity">
        <h3>叙事自我</h3>
        <div class="narrative-preview">
          <div class="narrative-line"><span class="narrative-label">自我认知</span> {{ data.narrative_self.core_identity || '—' }}</div>
          <div class="narrative-line" v-if="data.narrative_self.current_narrative"><span class="narrative-label">当前叙事</span> {{ data.narrative_self.current_narrative }}</div>
          <div class="narrative-stats">
            <span>💎 {{ data.narrative_self.stats?.values_count || 0 }} 价值观</span>
            <span>👁️ {{ data.narrative_self.stats?.concerns_count || 0 }} 关注</span>
            <span>📅 {{ data.narrative_self.stats?.timeline_count || 0 }} 时间线</span>
          </div>
        </div>
      </div>
    </div>

    <div class="card" v-if="data?.inner_thoughts?.length">
      <h3>内心独白 <span class="badge-sm">{{ data.inner_thoughts.length }}条</span></h3>
      <div class="thought-list">
        <div v-for="(t, i) in data.inner_thoughts" :key="i" class="thought-item" :class="{ faded: t.faded }">
          <div class="thought-content">{{ t.content }}</div>
          <div class="thought-meta">
            <span :style="{ color: t.emotional_impact > 0 ? 'var(--success)' : t.emotional_impact < 0 ? 'var(--danger)' : 'var(--text-3)' }">情绪 {{ t.emotional_impact.toFixed(1) }}</span>
            <span class="sep">·</span>
            <span>行动 {{ t.action_potential.toFixed(2) }}</span>
            <span class="sep">·</span>
            <span>{{ fmtRelTime(t.timestamp) }}</span>
            <span v-if="t.recall_count" class="sep">·</span>
            <span v-if="t.recall_count">想起 {{ t.recall_count }}次</span>
          </div>
        </div>
      </div>
    </div>

    <div v-if="!data || (!data.social_battery && !data.circadian && !data.attention)" class="card">
      <div class="empty">人性化功能未启用，请在配置中开启相关选项</div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const data = ref(null)
const labelMap = {
  social_battery: '社交电量', circadian: '昼夜节律', attention: '注意力',
  cognitive_biases: '认知偏差', satisficing: '满足性决策',
  response_timing: '变速回复', unpredictability: '不可预测性',
  inner_thought: '内心独白',
}

const rhythmBars = computed(() => {
  if (!data.value?.circadian) return []
  const c = data.value.circadian
  return [
    { label: '精力', value: c.energy_level, color: '#6366f1' },
    { label: '思维', value: c.cognitive_clarity, color: '#8b5cf6' },
    { label: '耐心', value: c.patience_level, color: '#34d399' },
    { label: '社交', value: c.sociability, color: '#f59e0b' },
    { label: '幽默', value: c.humor_sensitivity, color: '#f97316' },
  ]
})

const batteryColor = computed(() => {
  if (!data.value?.social_battery) return '#34d399'
  const p = data.value.social_battery.percentage
  if (p < 0.15) return '#ef4444'
  if (p < 0.3) return '#f97316'
  if (p < 0.5) return '#fbbf24'
  return '#34d399'
})

const stats = computed(() => {
  const d = data.value
  const I = {
    battery: '<svg viewBox="0 0 20 20" fill="none" width="22" height="22"><rect x="2" y="6" width="14" height="10" rx="2" stroke="currentColor" stroke-width="1.5"/><path d="M17 9v4M6 9v4M10 9v4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
    brain: '<svg viewBox="0 0 20 20" fill="none" width="22" height="22"><circle cx="10" cy="10" r="7" stroke="currentColor" stroke-width="1.5"/><path d="M7 8h6M7 11h4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
    heart: '<svg viewBox="0 0 20 20" fill="none" width="22" height="22"><path d="M10 17c-3-2-6-4.5-6-8a4 4 0 016-2.5A4 4 0 0116 9c0 3.5-3 6-6 8z" stroke="currentColor" stroke-width="1.5"/></svg>',
    thought: '<svg viewBox="0 0 20 20" fill="none" width="22" height="22"><path d="M10 2a7 7 0 00-7 7c0 1.5.5 2.9 1.3 4L3 17l4-1.3A7 7 0 1010 2z" stroke="currentColor" stroke-width="1.5" fill="none"/></svg>',
  }
  return [
    { label: '电量', value: d?.social_battery ? (d.social_battery.percentage * 100).toFixed(0) + '%' : '—', sub: '社交电量', color: '#34d399', icon: I.battery },
    { label: '注意力', value: d?.attention ? (d.attention.attention_level * 100).toFixed(0) + '%' : '—', sub: '注意力水平', color: '#8b5cf6', icon: I.brain },
    { label: '关系', value: d?.relationship_count ?? '—', sub: '已记录', color: '#ec4899', icon: I.heart },
    { label: '独白', value: d?.inner_thoughts?.length ?? '—', sub: '内心想法', color: '#f59e0b', icon: I.thought },
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

async function load() {
  try { data.value = await api('/api/humanity') } catch {}
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 16px; margin-bottom: 24px; }
.card h3 { font-size: 14px; font-weight: 600; margin-bottom: 12px; }
.stat-icon { margin-bottom: 8px; }
.stat-value { font-size: 26px; font-weight: 700; letter-spacing: -0.5px; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; font-weight: 500; }
.stat-sub { font-size: 11px; color: var(--text-3); margin-top: 2px; }
.card-row { display: flex; gap: 16px; }
.half { flex: 1; min-width: 0; }
.kv-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.kv { display: flex; justify-content: space-between; align-items: center; font-size: 13px; padding: 4px 0; border-bottom: 1px solid var(--border); }
.kv span { color: var(--text-2); }
.kv strong { font-weight: 600; }
.battery-visual { height: 24px; background: var(--surface); border-radius: 6px; overflow: hidden; margin-bottom: 12px; }
.battery-fill { height: 100%; border-radius: 6px; transition: width 0.8s ease, background 0.8s ease; min-width: 2px; }
.rhythm-bars { display: flex; flex-direction: column; gap: 6px; }
.rhythm-row { display: flex; align-items: center; gap: 8px; }
.rhythm-label { width: 32px; font-size: 11px; color: var(--text-2); flex-shrink: 0; }
.rhythm-bar-wrap { flex: 1; height: 8px; background: var(--surface); border-radius: 4px; overflow: hidden; }
.rhythm-bar { height: 100%; border-radius: 4px; transition: width 0.8s ease; }
.rhythm-pct { width: 36px; font-size: 11px; color: var(--text-2); text-align: right; flex-shrink: 0; }
.section-title { font-size: 13px; font-weight: 600; color: var(--text-2); margin-bottom: 8px; }
.toggle-grid { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 20px; }
.toggle-chip { padding: 4px 12px; border-radius: 20px; font-size: 11px; font-weight: 500; background: var(--surface); border: 1px solid var(--border); color: var(--text-3); }
.toggle-chip.on { background: var(--primary-glow); border-color: var(--primary); color: var(--primary); }
.motivation-badge { font-size: 14px; padding: 12px; background: var(--surface-hover); border-radius: var(--radius-sm); }
.thought-list { display: flex; flex-direction: column; gap: 10px; }
.thought-item { padding: 10px 14px; background: var(--surface-hover); border-radius: var(--radius-sm); border-left: 3px solid var(--primary); }
.thought-item.faded { opacity: 0.5; border-left-color: var(--text-3); }
.thought-content { font-size: 13px; line-height: 1.5; }
.thought-meta { display: flex; align-items: center; gap: 6px; margin-top: 6px; font-size: 11px; color: var(--text-3); }
.sep { color: var(--text-3); }
.badge-sm { font-size: 11px; padding: 2px 8px; border-radius: 10px; background: var(--primary-glow); color: var(--primary); font-weight: 500; margin-left: 6px; }
.empty { text-align: center; padding: 40px; color: var(--text-3); }
.narrative-preview { font-size: 13px; }
.narrative-line { margin-bottom: 6px; line-height: 1.5; }
.narrative-label { font-weight: 600; color: var(--text-2); margin-right: 6px; }
.narrative-stats { display: flex; gap: 12px; margin-top: 8px; font-size: 11px; color: var(--text-3); }
@media (max-width: 768px) { .card-row { flex-direction: column; } }
</style>
