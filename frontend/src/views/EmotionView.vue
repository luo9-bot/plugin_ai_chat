<template>
  <div>
    <h3>情绪状态</h3>
    <div class="toolbar">
      <select v-model="selectedUser" @change="loadDetail">
        <option value="">全部用户 ({{ Object.keys(states).length }})</option>
        <option v-for="(_, uid) in states" :key="uid" :value="uid">用户 {{ uid }}</option>
      </select>
    </div>
    <div v-if="selectedUser && detail">
      <div class="stat-grid">
        <div class="stat-card"><div class="label">当前情绪</div><div class="value">{{ emoIcon[detail.current] || '' }} {{ detail.current }}</div></div>
        <div class="stat-card"><div class="label">强度</div><div class="value">{{ (detail.intensity||0).toFixed(2) }}</div></div>
        <div class="stat-card"><div class="label">交互频率</div><div class="value">{{ (detail.interaction_rate||0).toFixed(2) }}</div></div>
        <div class="stat-card"><div class="label">危机等级</div><div class="value">{{ detail.crisis_level || 'None' }}</div></div>
      </div>
      <h3>📊 历史记录</h3>
      <div v-if="!(detail.history||[]).length" class="empty">暂无历史</div>
      <table v-else><thead><tr><th>情绪</th><th>时间</th></tr></thead><tbody>
        <tr v-for="([e, t], i) in (detail.history||[]).slice(-20).reverse()" :key="i">
          <td>{{ emoIcon[e] || '' }} {{ e }}</td><td class="mono">{{ fmtTime(t) }}</td>
        </tr>
      </tbody></table>
    </div>
    <div v-else>
      <div v-if="!Object.keys(states).length" class="empty">📭 暂无情绪数据</div>
      <table v-else><thead><tr><th>用户</th><th>情绪</th><th>强度</th><th>交互频率</th><th>危机</th></tr></thead><tbody>
        <tr v-for="(s, uid) in states" :key="uid">
          <td class="mono">{{ uid }}</td>
          <td>{{ emoIcon[s.current] || '' }} {{ s.current }}</td>
          <td>{{ (s.intensity||0).toFixed(2) }}</td>
          <td>{{ (s.interaction_rate||0).toFixed(2) }}</td>
          <td>{{ s.crisis_level || 'None' }}</td>
        </tr>
      </tbody></table>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const emoIcon = { Neutral: '😐', Happy: '😄', Sad: '😢', Thinking: '🤔', Surprised: '😲', Angry: '😡', Shy: '😳', Worried: '😟', Tired: '😫', Excited: '🤩' }
const states = ref({})
const selectedUser = ref('')
const detail = ref(null)
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
async function load() { states.value = await api('/api/emotion'); detail.value = null }
async function loadDetail() {
  if (!selectedUser.value) { detail.value = null; return }
  const d = await api('/api/emotion/' + selectedUser.value); detail.value = d.state
}
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 20px 0 8px; color: var(--text-dim); }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; }
.toolbar select { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 12px; margin-bottom: 20px; }
.stat-card { background: var(--surface); border: 2px solid var(--accent-light); border-radius: var(--radius); padding: 16px; text-align: center; box-shadow: var(--shadow); }
.stat-card .label { font-size: 11px; color: var(--text-dim); margin-bottom: 4px; text-transform: uppercase; }
.stat-card .value { font-size: 24px; font-weight: 700; background: linear-gradient(135deg, var(--accent), var(--purple)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
tr:hover { background: var(--surface2); }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
</style>
