<template>
  <div>
    <h2>📊 仪表盘</h2>
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-icon">🧠</div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.memory_entries ?? '-' }}</div>
          <div class="stat-label">记忆条目</div>
        </div>
        <div class="stat-sub">{{ stats.memory_users ?? 0 }} 个用户</div>
      </div>
      <div class="stat-card">
        <div class="stat-icon">😊</div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.sticker_count ?? '-' }}</div>
          <div class="stat-label">表情包</div>
        </div>
        <div class="stat-sub">已注册可用的表情</div>
      </div>
      <div class="stat-card">
        <div class="stat-icon">💭</div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.emotion_users ?? '-' }}</div>
          <div class="stat-label">情绪追踪</div>
        </div>
        <div class="stat-sub">有情绪状态的用户</div>
      </div>
      <div class="stat-card">
        <div class="stat-icon">💬</div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.active_groups ?? '-' }}</div>
          <div class="stat-label">活跃群聊</div>
        </div>
        <div class="stat-sub">正在进行的群聊</div>
      </div>
      <div class="stat-card">
        <div class="stat-icon">👤</div>
        <div class="stat-info">
          <div class="stat-value">{{ stats.active_users ?? '-' }}</div>
          <div class="stat-label">活跃私聊</div>
        </div>
        <div class="stat-sub">正在进行的私聊</div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const stats = ref({})
onMounted(async () => { stats.value = await api('/api/dashboard') })
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 20px; font-weight: 600; }
.stats-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 16px; }
.stat-card { background: var(--surface); border: 1.5px solid var(--border); border-radius: var(--radius); padding: 20px; box-shadow: var(--shadow); display: flex; flex-direction: column; gap: 8px; transition: transform .15s; }
.stat-card:hover { transform: translateY(-2px); }
.stat-icon { font-size: 28px; }
.stat-info { display: flex; align-items: baseline; gap: 8px; }
.stat-value { font-size: 32px; font-weight: 700; color: var(--accent); }
.stat-label { font-size: 14px; color: var(--text-dim); }
.stat-sub { font-size: 12px; color: var(--text-dim); }
</style>
