<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-if="quota.enabled !== undefined">
        <div class="stat-value" :style="{ color: quota.enabled ? 'var(--success)' : 'var(--text-3)' }">{{ quota.enabled ? '已启用' : '已禁用' }}</div>
        <div class="stat-label">配额系统</div>
        <div class="stat-sub">{{ quota.segment_minutes || 5 }} 分钟/段</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const quota = ref({})

async function load() {
  try {
    const d = await api('/api/quota')
    quota.value = { enabled: d.enabled, segment_minutes: d.segment_minutes, segments: d.segments || [] }
  } catch {}
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 16px; margin-bottom: 16px; }
.card { padding: 20px; }
.stat-value { font-size: 24px; font-weight: 700; }
.stat-label { font-size: 13px; color: var(--text-2); margin-top: 4px; }
.stat-sub { font-size: 11px; color: var(--text-3); }
</style>