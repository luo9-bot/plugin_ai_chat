<template>
  <div>
    <h2>📢 主动对话</h2>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:20px">
      <div>
        <h3>⚙️ 运行配置</h3>
        <table><tbody>
          <tr><td>📢 启用</td><td><input type="checkbox" :checked="cfg.enabled !== false" @change="cfg.enabled = $event.target.checked; saveCfg()" style="accent-color:var(--accent)" /></td></tr>
          <tr><td>🌙 免打扰开始</td><td><input type="number" :value="cfg.quiet_start ?? 23" @change="cfg.quiet_start = +$event.target.value; saveCfg()" min="0" max="23" style="width:60px;background:var(--surface2);border:1.5px solid var(--border);border-radius:8px;padding:4px 8px" /> 时</td></tr>
          <tr><td>☀️ 免打扰结束</td><td><input type="number" :value="cfg.quiet_end ?? 7" @change="cfg.quiet_end = +$event.target.value; saveCfg()" min="0" max="23" style="width:60px;background:var(--surface2);border:1.5px solid var(--border);border-radius:8px;padding:4px 8px" /> 时</td></tr>
          <tr><td>⏰ 主动间隔</td><td><input type="number" :value="cfg.interval ?? 7200" @change="cfg.interval = +$event.target.value; saveCfg()" min="60" style="width:80px;background:var(--surface2);border:1.5px solid var(--border);border-radius:8px;padding:4px 8px" /> 秒</td></tr>
        </tbody></table>
      </div>
      <div>
        <h3>👥 用户状态 ({{ Object.keys(states).length }})</h3>
        <div v-if="!Object.keys(states).length" class="empty">📭 暂无用户状态</div>
        <table v-else><thead><tr><th>用户</th><th>忽略</th><th>提醒</th></tr></thead><tbody>
          <tr v-for="(s, uid) in states" :key="uid">
            <td class="mono">{{ uid }}</td>
            <td>{{ s.ignore_count || 0 }}</td>
            <td class="truncate">{{ (s.pending_reminders || []).map(r => r.date + ' ' + r.description).join(', ') || '-' }}</td>
          </tr>
        </tbody></table>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const cfg = ref({})
const states = ref({})
async function load() { states.value = await api('/api/proactive'); cfg.value = await api('/api/proactive/config') }
async function saveCfg() { await api('/api/proactive/config', { method: 'PUT', body: JSON.stringify(cfg.value) }) }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 20px 0 8px; color: var(--text-dim); font-weight: 500; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: #fff; border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.truncate { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.empty { text-align: center; padding: 20px; color: var(--text-dim); }
</style>
