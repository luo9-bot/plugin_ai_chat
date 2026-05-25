<template>
  <div>
    <div class="stat-grid">
      <div class="glass-card" v-for="s in statCards" :key="s.label">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>
    <div class="glass-card">
      <div class="card-header"><h3>主动配置</h3></div>
      <div v-if="!config" class="empty">加载中...</div>
      <div v-else class="config-grid">
        <div class="config-item">
          <label>启用</label>
          <span class="toggle" :class="{ on: config.enabled }" @click="toggleEnabled">{{ config.enabled ? '🟢 已开启' : '🔴 已关闭' }}</span>
        </div>
        <div class="config-item">
          <label>免打扰时段</label>
          <span>{{ config.quiet_start || 23 }}:00 ~ {{ config.quiet_end || 7 }}:00</span>
        </div>
        <div class="config-item">
          <label>间隔</label>
          <span>{{ config.interval || 3600 }}秒</span>
        </div>
        <div class="config-item">
          <label>忽略阈值</label>
          <span>{{ config.max_ignore || 3 }}次</span>
        </div>
      </div>
    </div>
    <div class="glass-card">
      <div class="card-header"><h3>最近主动消息</h3></div>
      <div v-if="!log.length" class="empty">暂无记录</div>
      <div v-else class="log-list">
        <div v-for="(l, i) in log" :key="i" class="log-item">
          <span class="log-time">{{ fmtTime(l.time) }}</span>
          <span class="log-content">{{ l.content }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const config = ref(null)
const log = ref([])

const statCards = ref([{ label: '活跃用户', value: '-', sub: '主动对话', color: '#6366f1' }])

function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

async function load() {
  try {
    const d = await api('/api/proactive')
    config.value = d.config || {}
    log.value = (d.log || []).reverse()
    statCards.value[0].value = d.user_count || '-'
  } catch {}
}
function toggleEnabled() { /* placeholder */ }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.stat-value { font-size: 28px; font-weight: 700; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.config-grid { display: flex; flex-direction: column; gap: 12px; }
.config-item { display: flex; align-items: center; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid var(--glass-border); }
.config-item label { font-size: 12px; font-weight: 600; color: var(--text-2); }
.toggle { cursor: pointer; font-size: 13px; font-weight: 500; }
.log-list { display: flex; flex-direction: column; gap: 4px; max-height: 400px; overflow-y: auto; }
.log-item { display: flex; gap: 12px; padding: 6px 8px; border-radius: var(--radius-xs); font-size: 13px; }
.log-item:hover { background: var(--surface-hover); }
.log-time { color: var(--text-3); font-size: 12px; white-space: nowrap; font-family: monospace; }
.log-content { flex: 1; }
</style>