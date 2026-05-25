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
      <div class="card-header"><h3>表情包管理 <span class="badge">{{ stickers.length }}</span></h3>
        <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
      </div>
      <div v-if="!stickers.length" class="empty">暂无表情包</div>
      <div v-else class="sticker-grid">
        <div v-for="(s, i) in stickers" :key="i" class="sticker-card">
          <div class="sticker-img">
            <img :src="'/api/sticker/' + s.hash + '/image'" :alt="s.hash" loading="lazy" />
          </div>
          <div class="sticker-info">
            <span class="sticker-tag" v-if="s.tags">{{ s.tags }}</span>
            <div class="sticker-actions">
              <button class="btn btn-ghost btn-xs" @click="toggle(s.hash)">{{ s.enabled ? '禁用' : '启用' }}</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const stickers = ref([])
const statCards = ref([{ label: '表情包', value: '-', sub: '已注册', color: '#6366f1' }])

async function load() {
  try {
    const d = await api('/api/sticker')
    stickers.value = d.stickers || []
    statCards.value[0].value = stickers.value.length
  } catch {}
}
async function toggle(hash) { await api('/api/sticker/' + hash + '/toggle', { method: 'POST' }); load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.stat-value { font-size: 28px; font-weight: 700; }
.stat-label { font-size: 13px; color: var(--text-2); }
.stat-sub { font-size: 11px; color: var(--text-3); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.sticker-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 12px; }
.sticker-card { padding: 8px; border-radius: var(--radius-sm); background: var(--surface-hover); transition: var(--transition); }
.sticker-card:hover { transform: translateY(-2px); }
.sticker-img { width: 100%; aspect-ratio: 1; overflow: hidden; border-radius: var(--radius-xs); background: var(--surface); }
.sticker-img img { width: 100%; height: 100%; object-fit: contain; }
.sticker-info { display: flex; flex-direction: column; gap: 4px; margin-top: 6px; }
.sticker-tag { font-size: 10px; color: var(--text-2); }
</style>