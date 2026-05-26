<template>
  <div>
    <div class="glass-card">
      <div class="card-header">
        <h3>自我记忆 <span class="badge">{{ thoughts.length }} 条</span></h3>
        <div class="header-actions">
          <input v-model="search" placeholder="搜索..." class="glass-input" />
          <select v-model="catFilter" class="glass-select">
            <option value="">全部分类</option><option value="reflection">反思</option><option value="experience">经历</option><option value="plan">计划</option><option value="feeling">感受</option>
          </select>
          <input type="date" v-model="dateFrom" class="glass-input" style="width:130px" />
          <span class="sep">~</span>
          <input type="date" v-model="dateTo" class="glass-input" style="width:130px" />
          <button class="btn btn-primary btn-sm" @click="showAdd = true">＋添加</button>
          <a class="btn btn-ghost btn-sm" href="/api/self-thoughts/export" target="_blank">📥 导出</a>
        </div>
      </div>
      <div v-if="!filtered.length" class="empty">暂无自我记忆</div>
      <div v-else class="timeline">
        <div v-for="(t, i) in filtered" :key="t._idx" class="timeline-item">
          <div class="timeline-dot" :style="{ background: catColor(t.category) }"></div>
          <div class="timeline-card">
            <div class="tl-header">
              <span class="tl-tag" :style="{ background: catColor(t.category) + '22', color: catColor(t.category) }">{{ catLabel(t.category) }}</span>
              <span class="tl-time">{{ fmtTime(t.created) }}</span>
            </div>
            <div class="tl-content">{{ t.content }}</div>
            <div class="tl-actions">
              <button class="btn btn-ghost btn-xs" @click="del(t._idx)">删除</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="showAdd" class="modal-overlay" @click.self="showAdd = false">
      <div class="glass-card modal">
        <h3 style="margin-bottom:16px">添加自我记忆</h3>
        <label>分类</label><select v-model="addCategory" class="glass-select" style="margin-bottom:12px;width:100%">
          <option value="reflection">反思</option><option value="experience">经历</option><option value="plan">计划</option><option value="feeling">感受</option>
        </select>
        <label>内容</label><textarea v-model="addContent" class="glass-input" style="margin-bottom:16px;width:100%" rows="3"></textarea>
        <div class="modal-actions"><button class="btn btn-ghost" @click="showAdd = false">取消</button><button class="btn btn-primary" @click="addThought">保存</button></div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const thoughts = ref([])
const search = ref('')
const catFilter = ref('')
const dateFrom = ref('')
const dateTo = ref('')
const showAdd = ref(false)
const addContent = ref('')
const addCategory = ref('reflection')

function catColor(cat) {
  const map = { reflection: '#6366f1', experience: '#34d399', plan: '#fbbf24', feeling: '#f472b6' }
  return map[cat] || '#6366f1'
}
function catLabel(cat) {
  const map = { reflection: '反思', experience: '经历', plan: '计划', feeling: '感受' }
  return map[cat] || cat
}
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }

const filtered = computed(() => {
  let list = thoughts.value
  if (search.value) { const q = search.value.toLowerCase(); list = list.filter(t => (t.content || '').toLowerCase().includes(q)) }
  if (catFilter.value) list = list.filter(t => t.category === catFilter.value)
  if (dateFrom.value) { const ts = new Date(dateFrom.value).getTime() / 1000; list = list.filter(t => (t.created || 0) >= ts) }
  if (dateTo.value) { const ts = new Date(dateTo.value).getTime() / 1000 + 86400; list = list.filter(t => (t.created || 0) < ts) }
  return list
})

async function load() {
  try { const d = await api('/api/self-thoughts'); thoughts.value = (d.thoughts || []).map((t, i) => ({ ...t, _idx: i })).reverse() } catch {}
}
async function del(idx) { await api('/api/self-thoughts/' + idx, { method: 'DELETE' }); load() }
async function addThought() { if (!addContent.value) return; await api('/api/self-thoughts', { method: 'POST', body: JSON.stringify({ content: addContent.value, category: addCategory.value }) }); showAdd.value = false; addContent.value = ''; load() }

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; }
.header-actions { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
.sep { color: var(--text-3); font-size: 12px; }
.badge { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 20px; background: var(--primary-glow); color: var(--primary); }
.glass-input, .glass-select { padding: 6px 10px; border-radius: var(--radius-xs); border: 1px solid var(--glass-border); background: var(--surface); color: var(--text); font-size: 12px; outline: none; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.btn-xs { padding: 3px 8px; font-size: 11px; }
.empty { text-align: center; padding: 32px; color: var(--text-3); }
.timeline { position: relative; padding-left: 24px; }
.timeline::before { content: ''; position: absolute; left: 8px; top: 0; bottom: 0; width: 2px; background: var(--glass-border); }
.timeline-item { position: relative; margin-bottom: 12px; }
.timeline-dot { position: absolute; left: -20px; top: 16px; width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--bg); z-index: 1; }
.timeline-card { padding: 12px 16px; border-radius: var(--radius-sm); background: var(--surface-hover); }
.tl-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
.tl-tag { font-size: 10px; font-weight: 600; padding: 2px 8px; border-radius: 4px; }
.tl-time { font-size: 11px; color: var(--text-3); }
.tl-content { font-size: 13px; line-height: 1.5; }
.tl-actions { display: flex; gap: 6px; margin-top: 8px; }
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); z-index: 200; display: flex; align-items: center; justify-content: center; }
.modal { width: 420px; padding: 24px; }
.modal label { display: block; font-size: 12px; font-weight: 600; margin-bottom: 4px; color: var(--text-2); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>