<template>
  <div>
    <div class="section-grid">
      <div class="glass-card">
        <div class="card-header"><h3>Bot 配置</h3></div>
        <div v-if="!config" class="empty">加载中...</div>
        <div v-else class="config-grid">
          <div v-for="(v, k) in config" :key="k" class="config-item">
            <label>{{ k }}</label>
            <span class="mono">{{ typeof v === 'object' ? JSON.stringify(v).slice(0, 60) + '...' : String(v) }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const config = ref(null)
onMounted(async () => { try { config.value = await api('/api/config') } catch {} })
</script>
<style scoped>
.section-grid { display: grid; gap: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.config-grid { display: flex; flex-direction: column; gap: 4px; }
.config-item { display: flex; justify-content: space-between; padding: 6px 0; font-size: 13px; border-bottom: 1px solid var(--glass-border); }
.config-item label { color: var(--text-2); font-weight: 500; width: 180px; }
.mono { font-family: monospace; font-size: 12px; word-break: break-all; text-align: right; }
</style>