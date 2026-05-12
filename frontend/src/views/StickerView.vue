<template>
  <div>
    <h2>😊 表情包管理</h2>
    <div class="toolbar">
      <div class="filter-tabs">
        <button :class="['tab', { active: filter === 'all' }]" @click="filter = 'all'">全部 <span class="count">{{ stats.total }}</span></button>
        <button :class="['tab', { active: filter === 'builtin' }]" @click="filter = 'builtin'">内置 <span class="count">{{ builtinCount }}</span></button>
        <button :class="['tab', { active: filter === 'collected' }]" @click="filter = 'collected'">收集 <span class="count">{{ collectedCount }}</span></button>
        <button :class="['tab', { active: filter === 'banned' }]" @click="filter = 'banned'" v-if="bannedCount">封禁 <span class="count">{{ bannedCount }}</span></button>
      </div>
      <div class="toolbar-right">
        <span class="stat">已注册 <strong>{{ stats.registered }}</strong></span>
        <input v-model="search" placeholder="搜索表情..." class="search-input" />
      </div>
    </div>

    <div v-if="!filtered.length" class="empty">📭 暂无匹配的表情包</div>
    <div v-else class="grid">
      <div v-for="s in filtered" :key="s.hash" class="card" :class="{ banned: s.is_banned }">
        <div class="img-wrap">
          <img :src="`/api/sticker/image/${s.hash}`" :alt="s.description" loading="lazy" @error="onImgError" />
          <div class="badge-wrap">
            <span :class="['badge', s.is_builtin ? 'badge-builtin' : 'badge-collected']">
              {{ s.is_builtin ? '内置' : '收集' }}
            </span>
            <span v-if="s.is_banned" class="badge badge-banned">已封禁</span>
            <span v-else-if="!s.is_registered" class="badge badge-unreg">未注册</span>
            <span v-else class="badge badge-ok">可用</span>
          </div>
        </div>
        <div class="info">
          <div class="desc" :title="s.description">{{ s.description || '无描述' }}</div>
          <div class="emotions">
            <span v-for="e in s.emotions" :key="e" class="emotion-tag">{{ e }}</span>
          </div>
          <div class="meta">
            <span>使用 {{ s.query_count || 0 }} 次</span>
          </div>
        </div>
        <div class="actions">
          <button class="btn" :class="s.is_banned ? 'btn-ok' : 'btn-warn'" @click="toggleBan(s.hash)">
            {{ s.is_banned ? '解封' : '封禁' }}
          </button>
          <button class="btn btn-del" @click="remove(s.hash)">删除</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const data = ref({})
const filter = ref('all')
const search = ref('')

const stats = computed(() => data.value)
const allStickers = computed(() => data.value.stickers || [])
const builtinCount = computed(() => allStickers.value.filter(s => s.is_builtin).length)
const collectedCount = computed(() => allStickers.value.filter(s => !s.is_builtin).length)
const bannedCount = computed(() => allStickers.value.filter(s => s.is_banned).length)

const filtered = computed(() => {
  let list = allStickers.value
  if (filter.value === 'builtin') list = list.filter(s => s.is_builtin)
  else if (filter.value === 'collected') list = list.filter(s => !s.is_builtin)
  else if (filter.value === 'banned') list = list.filter(s => s.is_banned)
  if (search.value) {
    const q = search.value.toLowerCase()
    list = list.filter(s =>
      (s.description || '').toLowerCase().includes(q) ||
      s.emotions.some(e => e.toLowerCase().includes(q))
    )
  }
  return list
})

async function load() { data.value = await api('/api/sticker') }
async function toggleBan(hash) { await api('/api/sticker/' + hash, { method: 'POST' }); load() }
async function remove(hash) { if (!confirm('确定删除此表情包？')) return; await api('/api/sticker/' + hash, { method: 'DELETE' }); load() }
function onImgError(e) { e.target.style.display = 'none' }

onMounted(load)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }

.toolbar { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; flex-wrap: wrap; gap: 12px; }
.filter-tabs { display: flex; gap: 4px; background: var(--accent-light); border-radius: var(--radius); padding: 3px; }
.tab { background: none; border: none; padding: 7px 16px; border-radius: calc(var(--radius) - 3px); cursor: pointer; font-size: 13px; color: var(--text-dim); transition: all .15s; white-space: nowrap; }
.tab:hover { color: var(--accent); }
.tab.active { background: #fff; color: var(--accent); font-weight: 500; box-shadow: 0 1px 4px rgba(236,72,153,.15); }
.tab .count { display: inline-block; background: var(--accent-light); color: var(--accent); font-size: 11px; padding: 0 6px; border-radius: 8px; margin-left: 4px; font-weight: 500; }
.tab.active .count { background: var(--accent); color: #fff; }

.toolbar-right { display: flex; align-items: center; gap: 12px; }
.stat { font-size: 13px; color: var(--text-dim); }
.stat strong { color: var(--accent); }
.search-input { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 7px 12px; border-radius: var(--radius); font-size: 13px; outline: none; width: 180px; }
.search-input:focus { border-color: var(--accent); }

.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 16px; }
.card { background: var(--surface); border: 1.5px solid var(--border); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); transition: transform .15s, box-shadow .15s; display: flex; flex-direction: column; }
.card:hover { transform: translateY(-3px); box-shadow: 0 6px 20px rgba(236,72,153,.2); }
.card.banned { opacity: .55; filter: grayscale(.6); }
.card.banned:hover { opacity: .8; }

.img-wrap { position: relative; width: 100%; aspect-ratio: 1; background: #f5f5f5; display: flex; align-items: center; justify-content: center; overflow: hidden; }
.img-wrap img { width: 100%; height: 100%; object-fit: cover; }
.badge-wrap { position: absolute; top: 6px; left: 6px; display: flex; flex-direction: column; gap: 3px; }
.badge { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 10px; font-weight: 600; line-height: 1.4; }
.badge-builtin { background: #fee2e2; color: #ef4444; }
.badge-collected { background: #e0e7ff; color: #6366f1; }
.badge-banned { background: #fef3c7; color: #d97706; }
.badge-ok { background: #d1fae5; color: #059669; }
.badge-unreg { background: #f3e8ff; color: #a855f7; }

.info { padding: 10px 12px 6px; flex: 1; display: flex; flex-direction: column; gap: 6px; }
.desc { font-size: 12px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.emotions { display: flex; flex-wrap: wrap; gap: 3px; }
.emotion-tag { font-size: 10px; background: var(--accent-light); color: var(--accent); padding: 1px 7px; border-radius: 8px; }
.meta { font-size: 11px; color: var(--text-dim); }

.actions { display: flex; gap: 6px; padding: 6px 12px 10px; }
.btn { flex: 1; border: none; padding: 6px 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; }
.btn-warn { background: var(--warning-light); color: var(--warning); }
.btn-warn:hover { background: var(--warning); color: #fff; }
.btn-ok { background: var(--success-light); color: var(--success); }
.btn-ok:hover { background: var(--success); color: #fff; }
.btn-del { background: var(--danger-light); color: var(--danger); }
.btn-del:hover { background: var(--danger); color: #fff; }

.empty { text-align: center; padding: 60px 20px; color: var(--text-dim); font-size: 15px; }
</style>
