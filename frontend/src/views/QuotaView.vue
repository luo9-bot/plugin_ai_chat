<template>
  <div>
    <div class="section-grid">
      <div class="glass-card">
        <div class="card-header"><h3>配额详情</h3><button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button></div>
        <div v-if="!quotas" class="empty">加载中...</div>
        <div v-else>
          <div v-for="(v, k) in quotas" :key="k" class="quota-card">
            <div class="quota-id mono">{{ k }}</div>
            <div class="quota-bars">
              <div class="quota-row"><label>分钟</label><div class="bar-wrap"><div class="bar" :style="{ width: Math.min((v.minute || 0) / Math.max(v.max_per_minute || 20, 1) * 100, 100) + '%', background: 'var(--primary)' }"></div></div><span class="mono">{{ v.minute || 0 }}</span></div>
              <div class="quota-row"><label>小时</label><div class="bar-wrap"><div class="bar" :style="{ width: Math.min((v.hour || 0) / Math.max(v.max_per_hour || 200, 1) * 100, 100) + '%', background: '#8b5cf6' }"></div></div><span class="mono">{{ v.hour || 0 }}</span></div>
              <div class="quota-row"><label>今天</label><div class="bar-wrap"><div class="bar" :style="{ width: Math.min((v.day || 0) / Math.max(v.max_per_day || 500, 1) * 100, 100) + '%', background: '#34d399' }"></div></div><span class="mono">{{ v.day || 0 }}</span></div>
            </div>
          </div>
          <div v-if="!Object.keys(quotas).length" class="empty" style="padding:24px">暂无配额数据</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const quotas = ref({})

async function load() {
  try {
    const d = await api('/api/quota')
    quotas.value = d.quotas || d
  } catch {}
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.section-grid { display: grid; gap: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.quota-card { padding: 12px; border-radius: var(--radius-sm); background: var(--surface-hover); margin-bottom: 8px; }
.quota-id { font-size: 13px; font-weight: 600; margin-bottom: 8px; }
.quota-bars { display: flex; flex-direction: column; gap: 6px; }
.quota-row { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.quota-row label { width: 32px; color: var(--text-2); }
.bar-wrap { flex: 1; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; }
.bar { height: 100%; border-radius: 3px; transition: width 0.5s ease; }
.mono { font-family: monospace; font-size: 12px; color: var(--text-2); min-width: 32px; text-align: right; }
</style>