<template>
  <div>
    <div class="section-grid">
      <div class="glass-card">
        <div class="card-header"><h3>人格模板</h3></div>
        <div v-if="!templates" class="empty">加载中...</div>
        <div v-else>
          <div class="chip-list">
            <div v-for="(t, i) in templates" :key="i" class="chip" @click="switchTo(t)">{{ t }}</div>
          </div>
        </div>
      </div>
      <div class="glass-card">
        <div class="card-header"><h3>当前人格特质</h3></div>
        <div v-if="!current" class="empty">加载中...</div>
        <div v-else class="config-grid">
          <div v-for="(v, k) in current" :key="k" class="config-item">
            <label>{{ k }}</label>
            <div class="bar-wrap"><div class="bar" :style="{ width: (v * 100) + '%' }"></div></div>
            <span class="mono">{{ typeof v === 'number' ? v.toFixed(2) : v }}</span>
          </div>
        </div>
        <div style="display:flex;gap:8px;margin-top:12px">
          <input v-model="snapName" placeholder="快照名称" class="glass-input" style="flex:1" />
          <button class="btn btn-primary" @click="saveSnap">保存快照</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const templates = ref([])
const current = ref(null)
const snapshots = ref([])
const snapName = ref('')

async function load() {
  try {
    const d = await api('/api/personality')
    current.value = d.current || {}
    snapshots.value = d.snapshots ? Object.keys(d.snapshots) : []
    // Try to extract template names from personality.json or defaults
    templates.value = snapshots.value.length ? snapshots.value : ['default']
  } catch {}
}
async function switchTo(t) {
  try { await api('/api/personality/load/' + encodeURIComponent(t), { method: 'POST' }) } catch (e) { alert('切换失败: ' + e.message) }
  load()
}
async function saveSnap() {
  if (!snapName.value) return
  try { await api('/api/personality/save/' + encodeURIComponent(snapName.value), { method: 'POST' }); snapName.value = ''; load() }
  catch (e) { alert('保存失败: ' + e.message) }
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.section-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 16px; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); }
.card-header h3 { font-size: 15px; font-weight: 600; margin-bottom: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.chip-list { display: flex; flex-wrap: wrap; gap: 8px; }
.chip { padding: 6px 14px; background: var(--surface-hover); border-radius: 20px; font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.chip:hover { background: var(--primary); color: white; }
.config-grid { display: flex; flex-direction: column; gap: 8px; }
.config-item { display: flex; align-items: center; gap: 8px; padding: 4px 0; font-size: 13px; }
.config-item label { color: var(--text-2); width: 100px; white-space: nowrap; }
.bar-wrap { flex: 1; height: 6px; background: var(--surface); border-radius: 3px; overflow: hidden; }
.bar { height: 100%; background: var(--primary); border-radius: 3px; transition: width 0.5s ease; }
.mono { font-size: 12px; color: var(--text-2); width: 48px; text-align: right; }
.glass-input { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--glass-border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
</style>