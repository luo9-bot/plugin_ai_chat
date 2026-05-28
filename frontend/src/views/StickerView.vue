<template>
  <div>
    <div class="stat-grid">
      <div class="card" v-for="s in statCards" :key="s.label">
        <div class="stat-value" :style="{ color: s.color }">{{ s.value }}</div>
        <div class="stat-label">{{ s.label }}</div>
        <div class="stat-sub">{{ s.sub }}</div>
      </div>
    </div>
    <div class="card">
      <div class="card-header">
        <h3>表情包 <span class="badge">{{ stickers.length }}</span></h3>
        <div class="header-actions">
          <input v-model="search" placeholder="搜索标签..." class="glass-input" />
          <button class="btn btn-ghost btn-sm" @click="load">↻ 刷新</button>
        </div>
      </div>
      <div v-if="!stickers.length" class="empty">暂无表情包</div>
      <div v-else class="sticker-grid">
        <div v-for="(s, i) in filtered" :key="i" class="sticker-card">
          <div class="sticker-img">
            <img :src="'/api/sticker/image/' + s.hash" :alt="s.hash" loading="lazy" @error="$event.target.style.display='none'" />
          </div>
          <div class="sticker-info">
            <div class="sticker-tags" v-if="s.description || s.vlm_description">
              <span class="chip-sm">{{ (s.description || s.vlm_description || '').slice(0, 30) }}</span>
            </div>
            <span v-else class="text-muted">无描述</span>
            <div class="sticker-actions">
              <span class="badge-sm" :class="s.is_banned ? 'off' : 'on'">{{ s.is_banned ? '禁用' : '启用' }}</span>
              <button class="btn btn-ghost btn-xs" @click="toggle(s.hash)">{{ s.is_banned ? '启用' : '禁用' }}</button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const stickers = ref([])
const search = ref('')

const statCards = ref([{ label: '表情包', value: '-', sub: '已注册', color: '#6366f1' }])

const filtered = computed(() => {
  if (!search.value) return stickers.value
  const q = search.value.toLowerCase()
  return stickers.value.filter(s => {
    const desc = ((s.description || s.vlm_description || '') + ' ' + s.hash).toLowerCase()
    return desc.includes(q)
  })
})

async function load() {
  try {
    const d = await api('/api/sticker')
    stickers.value = d.stickers || []
    statCards.value[0].value = stickers.value.length
  } catch {}
}
async function toggle(hash) { await api('/api/sticker/' + hash, { method: 'POST' }); load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 16px; margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 8px; }
.glass-input { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.stat-value { font-size: 28px; font-weight: 700; }
.stat-label { font-size: 13px; color: var(--text-2); }
.stat-sub { font-size: 11px; color: var(--text-3); }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.sticker-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); gap: 12px; }
.sticker-card { padding: 8px; border-radius: var(--radius-sm); background: var(--surface-hover); transition: var(--transition); }
.sticker-card:hover { transform: translateY(-2px); }
.sticker-img { width: 100%; aspect-ratio: 1; overflow: hidden; border-radius: var(--radius-xs); background: var(--surface); }
.sticker-img img { width: 100%; height: 100%; object-fit: contain; }
.sticker-info { display: flex; flex-direction: column; gap: 4px; margin-top: 6px; }
.sticker-tags { display: flex; flex-wrap: wrap; gap: 3px; }
.chip-sm { font-size: 10px; padding: 1px 6px; background: var(--surface); border-radius: 3px; color: var(--text-2); }
.text-muted { font-size: 10px; color: var(--text-3); }
.sticker-actions { display: flex; align-items: center; justify-content: space-between; }
.badge-sm { font-size: 10px; font-weight: 500; padding: 1px 6px; border-radius: 3px; }
.badge-sm.on { background: rgba(52,211,153,0.15); color: var(--success); }
.badge-sm.off { background: rgba(239,68,68,0.1); color: var(--danger); }
</style>