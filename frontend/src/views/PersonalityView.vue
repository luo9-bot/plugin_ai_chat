<template>
  <div>
    <h2>🎭 人格设定</h2>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:20px">
      <div>
        <h3>👤 当前: {{ cur.name }}</h3>
        <table><tbody>
          <tr><td>🎭 模板</td><td><strong>{{ cur.template }}</strong></td></tr>
          <tr v-for="(v, k) in cur.traits || {}" :key="k">
            <td>{{ traitLabels[k] || k }}</td>
            <td style="display:flex;align-items:center;gap:8px">
              <input type="range" min="0" max="1" step="0.05" :value="v" @change="updateTrait(k, $event.target.value)" style="width:100px;accent-color:var(--accent)" />
              <span class="mono">{{ parseFloat(v).toFixed(2) }}</span>
            </td>
          </tr>
        </tbody></table>
        <label style="margin-top:14px;display:block;font-size:12px;color:var(--text-dim)">📝 自定义提示词</label>
        <textarea v-model="customPrompt" style="width:100%;min-height:80px;background:var(--surface2);border:1.5px solid var(--border);color:var(--text);padding:10px;border-radius:var(--radius);font-size:13px;outline:none"></textarea>
        <button class="btn btn-primary" style="margin-top:8px" @click="savePrompt">💾 保存提示词</button>
      </div>
      <div>
        <h3>📋 快照管理</h3>
        <div class="toolbar"><button class="btn btn-primary btn-sm" @click="showSaveSnap = true">💾 保存当前为快照</button></div>
        <div v-if="!snapNames.length" class="empty">📭 暂无快照</div>
        <div v-for="n in snapNames" :key="n" style="display:flex;align-items:center;gap:8px;padding:10px;margin-bottom:6px;background:var(--surface2);border-radius:var(--radius)">
          <span style="flex:1;font-weight:500">{{ n }}</span>
          <button class="btn btn-outline btn-sm" @click="loadSnap(n)">加载</button>
          <button class="btn btn-danger btn-sm" @click="delSnap(n)">删除</button>
        </div>
      </div>
    </div>
    <div v-if="showSaveSnap" class="modal-overlay" @click.self="showSaveSnap = false">
      <div class="modal"><h3>💾 保存人格快照</h3>
        <label>名称</label><input v-model="snapName" placeholder="给快照取个名字..." />
        <div class="modal-actions"><button class="btn btn-outline" @click="showSaveSnap = false">取消</button><button class="btn btn-primary" @click="doSaveSnap">保存</button></div>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'
const traitLabels = { humor: '😂 幽默', warmth: '❤️ 温暖', curiosity: '🔬 好奇', formality: '🎨 正式', verbosity: '📝 详细', empathy: '🤗 共情' }
const cur = ref({ name: '', template: '', traits: {}, custom_prompt: '' })
const snapshots = ref({})
const customPrompt = ref('')
const showSaveSnap = ref(false)
const snapName = ref('')
const snapNames = computed(() => Object.keys(snapshots.value))
async function load() { const d = await api('/api/personality'); cur.value = d.current || {}; snapshots.value = d.snapshots || {}; customPrompt.value = cur.value.custom_prompt || '' }
async function updateTrait(k, v) { cur.value.traits[k] = parseFloat(v); await api('/api/personality', { method: 'PUT', body: JSON.stringify({ current: cur.value }) }) }
async function savePrompt() { cur.value.custom_prompt = customPrompt.value; await api('/api/personality', { method: 'PUT', body: JSON.stringify({ current: cur.value }) }) }
async function doSaveSnap() { if (!snapName.value.trim()) return; await api('/api/personality/snapshots', { method: 'POST', body: JSON.stringify({ name: snapName.value.trim() }) }); showSaveSnap.value = false; snapName.value = ''; load() }
async function loadSnap(n) { if (!confirm(`加载快照"${n}"？当前人格将被覆盖。`)) return; await api('/api/personality/snapshots/' + encodeURIComponent(n) + '/load', { method: 'POST' }); load() }
async function delSnap(n) { if (!confirm(`删除快照"${n}"？`)) return; await api('/api/personality/snapshots/' + encodeURIComponent(n), { method: 'DELETE' }); load() }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 20px 0 8px; color: var(--text-dim); font-weight: 500; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: #fff; border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 20px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn-primary { background: linear-gradient(135deg, var(--accent), var(--purple)); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: #fff; border: 1.5px solid var(--border); color: var(--accent); }
.btn-outline:hover { background: var(--accent-light); }
.btn-sm { padding: 5px 12px; font-size: 11px; }
.modal-overlay { position: fixed; inset: 0; background: rgba(74,53,72,.4); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; z-index: 100; }
.modal { background: #fff; border: 2px solid var(--border); border-radius: 16px; padding: 28px; width: 400px; max-width: 90vw; box-shadow: 0 20px 60px rgba(236,72,153,.15); }
.modal h3 { margin-top: 0; font-size: 16px; color: var(--accent); }
.modal label { display: block; margin: 14px 0 4px; font-size: 12px; color: var(--text-dim); font-weight: 500; }
.modal input { width: 100%; background: var(--surface2); border: 1.5px solid var(--border); color: var(--text); padding: 10px 14px; border-radius: var(--radius); font-size: 13px; outline: none; }
.modal input:focus { border-color: var(--accent); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 24px; }
</style>
