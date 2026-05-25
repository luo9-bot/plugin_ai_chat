<template>
  <div>
    <div class="glass-card">
      <div class="card-header"><h3>归档统计</h3></div>
      <div v-if="!stats" class="empty">加载中...</div>
      <div v-else>
        <div class="stat-row">
          <div class="glass-inner stat-item"><span class="stat-num">{{ stats.working_memory || 0 }}</span><span class="stat-lbl">工作记忆</span></div>
          <div class="glass-inner stat-item"><span class="stat-num">{{ stats.long_term || 0 }}</span><span class="stat-lbl">长期归档</span></div>
        </div>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const stats = ref(null)
onMounted(async () => { try { stats.value = await api('/api/archive') } catch {} })
</script>
<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.stat-row { display: flex; gap: 16px; }
.stat-item { flex: 1; text-align: center; padding: 24px; border-radius: var(--radius-sm); background: var(--surface-hover); }
.stat-num { font-size: 32px; font-weight: 700; color: var(--primary); }
.stat-lbl { display: block; font-size: 13px; color: var(--text-2); margin-top: 4px; }
</style>