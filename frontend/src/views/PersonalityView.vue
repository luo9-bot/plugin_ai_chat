<template>
  <div>
    <h2>🎭 人格设定</h2>

    <div class="grid">
      <!-- 左：当前人格 -->
      <div class="section">
        <h3>👤 当前人格: <strong>{{ cur.name || 'default' }}</strong></h3>
        <div class="template-badge">模板: {{ cur.template || 'default' }}</div>

        <h4>特质参数</h4>
        <div class="traits">
          <div class="trait-row" v-for="(v, k) in cur.traits || {}" :key="k">
            <span class="trait-label">{{ traitLabels[k] || k }}</span>
            <input type="range" min="0" max="1" step="0.05" :value="v"
                   @change="updateTrait(k, $event.target.value)" />
            <span class="trait-val">{{ parseFloat(v).toFixed(2) }}</span>
          </div>
        </div>

        <h4>📝 自定义提示词</h4>
        <textarea v-model="customPrompt" rows="4" placeholder="在此输入额外的人格描述..."></textarea>
        <button class="btn btn-primary" @click="savePrompt">💾 保存提示词</button>
      </div>

      <!-- 右：快照管理 -->
      <div class="section">
        <h3>📋 快照管理</h3>
        <p class="desc">保存当前人格配置为快照，可随时切换</p>
        <button class="btn btn-primary btn-sm" @click="showSaveSnap = true" style="margin-bottom:12px">
          💾 保存当前为快照
        </button>

        <div v-if="!snapNames.length" class="empty">暂无快照</div>
        <div v-for="n in snapNames" :key="n" class="snap-row">
          <span class="snap-name">{{ n }}</span>
          <button class="btn btn-outline btn-sm" @click="loadSnap(n)">加载</button>
          <button class="btn btn-danger btn-sm" @click="delSnap(n)">删除</button>
        </div>
      </div>
    </div>

    <!-- 保存快照弹窗 -->
    <div v-if="showSaveSnap" class="modal-overlay" @click.self="showSaveSnap = false">
      <div class="modal">
        <h3>💾 保存人格快照</h3>
        <label>名称</label>
        <input v-model="snapName" placeholder="给快照取个名字..." @keydown.enter="doSaveSnap" />
        <div class="modal-actions">
          <button class="btn btn-outline" @click="showSaveSnap = false">取消</button>
          <button class="btn btn-primary" @click="doSaveSnap" :disabled="!snapName.trim()">保存</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'

const traitLabels = {
  humor: '😂 幽默', warmth: '❤️ 温暖', curiosity: '🔬 好奇',
  formality: '🎨 正式', verbosity: '📝 详细', empathy: '🤗 共情',
}

const cur = ref({ name: 'default', template: 'default', traits: {}, custom_prompt: '' })
const snapshots = ref({})
const customPrompt = ref('')
const showSaveSnap = ref(false)
const snapName = ref('')
const snapNames = computed(() => Object.keys(snapshots.value))

async function load() {
  try {
    const d = await api('/api/personality')
    cur.value = d.current || { name: 'default', template: 'default', traits: {}, custom_prompt: '' }
    snapshots.value = d.snapshots || {}
    customPrompt.value = cur.value.custom_prompt || ''
  } catch (e) {
    console.error('Failed to load personality:', e)
  }
}

async function updateTrait(k, v) {
  cur.value.traits[k] = parseFloat(v)
  await api('/api/personality', { method: 'PUT', body: JSON.stringify({ current: cur.value }) })
}

async function savePrompt() {
  cur.value.custom_prompt = customPrompt.value
  await api('/api/personality', { method: 'PUT', body: JSON.stringify({ current: cur.value }) })
}

async function doSaveSnap() {
  if (!snapName.value.trim()) return
  await api('/api/personality/snapshots', { method: 'POST', body: JSON.stringify({ name: snapName.value.trim() }) })
  showSaveSnap.value = false
  snapName.value = ''
  load()
}

async function loadSnap(n) {
  if (!confirm(`加载快照"${n}"？当前人格将被覆盖。`)) return
  await api('/api/personality/snapshots/' + encodeURIComponent(n) + '/load', { method: 'POST' })
  load()
}

async function delSnap(n) {
  if (!confirm(`删除快照"${n}"？`)) return
  await api('/api/personality/snapshots/' + encodeURIComponent(n), { method: 'DELETE' })
  load()
}

onMounted(load)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 0 0 12px; font-weight: 600; color: var(--text); }
h4 { font-size: 13px; margin: 16px 0 8px; font-weight: 600; color: var(--text-dim); }
.desc { font-size: 12px; color: var(--text-dim); margin: -6px 0 12px; }

.grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
@media (max-width: 768px) { .grid { grid-template-columns: 1fr; } }

.section {
  background: var(--surface); border-radius: var(--radius); padding: 16px;
  box-shadow: var(--shadow);
}

.template-badge {
  display: inline-block; background: var(--accent-light); color: var(--accent);
  padding: 4px 12px; border-radius: 20px; font-size: 12px; font-weight: 500;
  margin-bottom: 8px;
}

.traits { display: flex; flex-direction: column; gap: 10px; }
.trait-row {
  display: flex; align-items: center; gap: 12px;
  padding: 8px 12px; background: var(--accent-light); border-radius: var(--radius);
}
.trait-label { width: 80px; font-size: 13px; font-weight: 500; }
.trait-row input[type="range"] { flex: 1; accent-color: var(--accent); }
.trait-val { width: 40px; text-align: right; font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; font-weight: 600; color: var(--accent); }

textarea {
  width: 100%; background: var(--accent-light); border: 1.5px solid var(--border);
  color: var(--text); padding: 10px; border-radius: var(--radius);
  font-size: 13px; outline: none; resize: vertical; margin-bottom: 8px;
}
textarea:focus { border-color: var(--accent); }

.empty { text-align: center; padding: 20px; color: var(--text-dim); font-size: 13px; }

.snap-row {
  display: flex; align-items: center; gap: 8px;
  padding: 10px 14px; background: var(--accent-light); border-radius: var(--radius);
  margin-bottom: 6px;
}
.snap-name { flex: 1; font-weight: 500; font-size: 14px; }

.btn {
  border: none; padding: 8px 16px; border-radius: var(--radius);
  cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s;
  display: inline-flex; align-items: center; gap: 4px;
}
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background: var(--accent); color: #fff; }
.btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: var(--surface); border: 1.5px solid var(--border); color: var(--accent); }
.btn-outline:hover { background: var(--accent-light); }
.btn-sm { padding: 5px 12px; font-size: 11px; }

.modal-overlay { position: fixed; inset: 0; background: rgba(74,53,72,.4); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; z-index: 100; }
.modal { background: var(--surface); border: 2px solid var(--border); border-radius: 16px; padding: 28px; width: 400px; max-width: 90vw; box-shadow: 0 20px 60px rgba(236,72,153,.15); }
.modal h3 { margin-top: 0; font-size: 16px; color: var(--accent); }
.modal label { display: block; margin: 14px 0 4px; font-size: 12px; color: var(--text-dim); font-weight: 500; }
.modal input { width: 100%; background: var(--accent-light); border: 1.5px solid var(--border); color: var(--text); padding: 10px 14px; border-radius: var(--radius); font-size: 13px; outline: none; }
.modal input:focus { border-color: var(--accent); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 24px; }
</style>
