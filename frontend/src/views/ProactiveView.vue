<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="s in statCards" :key="s.label">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>

    <div class="card" v-if="motivation">
      <div class="card-header"><h3>🎯 动机系统</h3></div>
      <div class="motivation-grid">
        <div class="motivation-item" v-for="m in motivationList" :key="m.label">
          <span class="motivation-label">{{ m.label }}</span>
          <div class="motivation-bar-wrap"><div class="motivation-bar" :style="{ width: m.value * 100 + '%', background: m.color }"></div></div>
          <span class="motivation-pct">{{ (m.value * 100).toFixed(0) }}%</span>
        </div>
      </div>
      <div class="motivation-current" v-if="motivation.type">
        当前最强动机: <strong>{{ motivation.type }}</strong> ({{ (motivation.strength * 100).toFixed(0) }}%)
      </div>
    </div>

    <div class="card">
      <div class="card-header">
        <h3>主动消息配置</h3>
        <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
      </div>
      <div v-if="!config" class="empty">加载中...</div>
      <div v-else class="config-grid">
        <div class="config-item">
          <label>启用</label>
          <div class="config-val">
            <button class="toggle-btn" :class="{ on: isEnabled }" @click="toggleEnabled">
              <span class="toggle-knob"></span>
            </button>
            <span>{{ isEnabled ? '已开启' : '已关闭' }}</span>
            <span v-if="config.enabled === null" class="hint">(使用默认)</span>
          </div>
        </div>
        <div class="config-item">
          <label>免打扰时段</label>
          <span>
            {{ config.quiet_start != null ? config.quiet_start : '默认' }}:00 — {{ config.quiet_end != null ? config.quiet_end : '默认' }}:00
            <span v-if="config.quiet_start == null" class="hint">(使用全局配置)</span>
          </span>
        </div>
        <div class="config-item">
          <label>间隔</label>
          <span>
            {{ config.interval != null ? config.interval + ' 秒' : '使用全局配置' }}
          </span>
        </div>
        <div class="config-item" v-if="config.group_last_sent">
          <label>群最近发送</label>
          <span>{{ Object.keys(config.group_last_sent).length }} 个群有记录</span>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-header"><h3>用户状态</h3></div>
      <div v-if="!userStates.length" class="empty">暂无用户数据</div>
      <div v-else class="table-wrap">
        <table>
          <thead><tr><th>用户 ID</th><th>最后发送</th><th>最后回复</th><th>忽略次数</th><th>状态</th></tr></thead>
          <tbody>
            <tr v-for="(u, i) in userStates" :key="i">
              <td class="mono">{{ u.uid }}</td>
              <td class="mono">{{ u.last_sent ? fmtTime(u.last_sent) : '-' }}</td>
              <td class="mono">{{ u.last_reply ? fmtTime(u.last_reply) : '-' }}</td>
              <td class="mono">{{ u.ignore_count }}</td>
              <td><span class="badge-state" :class="u.ignore_count > 3 ? 'tired' : 'ok'">{{ u.ignore_count > 3 ? '冷却中' : '正常' }}</span></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const config = ref(null)
const userStates = ref([])
const isEnabled = ref(false)
const motivation = ref(null)

const statCards = ref([{ label: '活跃用户', value: '-', sub: '主动对话', color: '#6366f1' }])

const motivationList = computed(() => {
  if (!motivation.value) return []
  // 从 motivation 数据中提取各维度
  const m = motivation.value
  return [
    { label: '分享欲', value: m.urge_to_share || 0, color: '#6366f1' },
    { label: '关心欲', value: m.caring_check_in || 0, color: '#ec4899' },
    { label: '延续话题', value: m.open_thread_pull || 0, color: '#f59e0b' },
    { label: '表达欲', value: m.accumulated_expression || 0, color: '#34d399' },
    { label: '社交需求', value: m.social_need || 0, color: '#8b5cf6' },
    { label: '好奇心', value: m.curiosity_drive || 0, color: '#06b6d4' },
  ]
})

function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

async function load() {
  try {
    const d = await api('/api/proactive')
    // d is a map of user_id -> state
    if (d && typeof d === 'object' && !Array.isArray(d)) {
      userStates.value = Object.entries(d).map(([uid, s]) => ({
        uid,
        last_sent: s.last_sent,
        last_reply: s.last_user_reply,
        ignore_count: s.ignore_count || 0,
      }))
      statCards.value[0].value = userStates.value.length
    }
    // Load config separately
    try {
      config.value = await api('/api/proactive/config')
      isEnabled.value = config.value?.enabled != null ? config.value.enabled : true
    } catch {}
    // Load motivation data from proactive motivation file
    try {
      const h = await api('/api/humanity')
      motivation.value = h.motivation || null
    } catch {}
  } catch {}
}

async function toggleEnabled() {
  if (!config.value) return
  const newVal = !config.value.enabled
  try {
    await api('/api/proactive/config', {
      method: 'PUT',
      body: JSON.stringify({ enabled: newVal })
    })
    config.value.enabled = newVal
  } catch (e) {
    alert('切换失败: ' + e.message)
  }
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.stat-value { font-size: 28px; font-weight: 700; }
.stat-label { font-size: 13px; color: var(--text-2); }
.stat-sub { font-size: 11px; color: var(--text-3); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.config-grid { display: flex; flex-direction: column; gap: 4px; }
.config-item { display: flex; justify-content: space-between; align-items: center; padding: 10px 0; font-size: 13px; border-bottom: 1px solid var(--border); }
.config-item label { color: var(--text-2); font-weight: 500; }
.config-val { display: flex; align-items: center; gap: 8px; }
.toggle-btn { width: 40px; height: 22px; border-radius: 11px; border: none; background: var(--text-3); cursor: pointer; position: relative; transition: var(--transition); padding: 0; }
.toggle-btn.on { background: var(--primary); }
.toggle-knob { position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: white; transition: var(--transition); }
.toggle-btn.on .toggle-knob { left: 20px; }
.table-wrap { overflow-x: auto; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th { text-align: left; padding: 8px 12px; font-weight: 600; font-size: 11px; color: var(--text-3); text-transform: uppercase; border-bottom: 1px solid var(--border); }
td { padding: 8px 12px; border-bottom: 1px solid var(--border); }
tr:hover td { background: var(--surface-hover); }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); }
.badge-state { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 4px; }
.badge-state.ok { background: rgba(52,211,153,0.15); color: var(--success); }
.badge-state.tired { background: rgba(251,191,36,0.15); color: var(--warning); }
.hint { font-size: 11px; color: var(--text-3); margin-left: 4px; }
.motivation-grid { display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px; }
.motivation-item { display: flex; align-items: center; gap: 8px; }
.motivation-label { width: 56px; font-size: 11px; color: var(--text-2); flex-shrink: 0; }
.motivation-bar-wrap { flex: 1; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; }
.motivation-bar { height: 100%; border-radius: 3px; transition: width 0.5s ease; }
.motivation-pct { width: 36px; font-size: 11px; color: var(--text-3); text-align: right; flex-shrink: 0; }
.motivation-current { font-size: 13px; padding: 8px 12px; background: var(--surface-hover); border-radius: var(--radius-xs); }
</style>