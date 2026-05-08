<template>
  <div>
    <h2>🧬 心理状态</h2>
    <h3>😟 担忧 ({{ concerns.length }})</h3>
    <div class="toolbar"><button class="btn btn-primary btn-sm" @click="showConcern = true">＋ 添加担忧</button></div>
    <div v-if="!concerns.length" class="empty">😊 暂无担忧</div>
    <table v-else><thead><tr><th>内容</th><th>分类</th><th>强度</th><th>时间</th><th>操作</th></tr></thead><tbody>
      <tr v-for="(c, i) in concerns" :key="i">
        <td class="truncate">{{ c.content }}</td>
        <td><span class="tag">{{ c.category }}</span></td>
        <td>{{ (c.strength||0).toFixed(2) }}</td>
        <td class="mono">{{ fmtTime(c.created) }}</td>
        <td><button class="btn btn-danger btn-sm" @click="delConcern(i)">删除</button></td>
      </tr>
    </tbody></table>
    <h3>💭 考量 ({{ deliberations.length }})</h3>
    <div class="toolbar"><button class="btn btn-primary btn-sm" @click="showDelib = true">＋ 添加考量</button></div>
    <div v-if="!deliberations.length" class="empty">😊 暂无考量</div>
    <table v-else><thead><tr><th>内容</th><th>来源</th><th>强度</th><th>时间</th><th>操作</th></tr></thead><tbody>
      <tr v-for="(d, i) in deliberations" :key="i">
        <td class="truncate">{{ d.content }}</td>
        <td><span class="tag">{{ d.source }}</span></td>
        <td>{{ (d.strength||0).toFixed(2) }}</td>
        <td class="mono">{{ fmtTime(d.created) }}</td>
        <td><button class="btn btn-danger btn-sm" @click="delDelib(i)">删除</button></td>
      </tr>
    </tbody></table>
    <div v-if="showConcern" class="modal-overlay" @click.self="showConcern = false">
      <div class="modal"><h3>＋ 添加担忧</h3>
        <label>内容</label><textarea v-model="cContent" placeholder="担忧内容..."></textarea>
        <label>分类</label><select v-model="cCategory"><option value="social">👥 社交</option><option value="task">📋 任务</option><option value="emotional">💗 情感</option><option value="self_">🧠 自我</option></select>
        <label>强度 (0-1)</label><input type="number" v-model.number="cStrength" min="0" max="1" step="0.1" />
        <div class="modal-actions"><button class="btn btn-outline" @click="showConcern = false">取消</button><button class="btn btn-primary" @click="addConcern">保存</button></div>
      </div>
    </div>
    <div v-if="showDelib" class="modal-overlay" @click.self="showDelib = false">
      <div class="modal"><h3>＋ 添加考量</h3>
        <label>内容</label><textarea v-model="dContent" placeholder="考量内容..."></textarea>
        <label>来源</label><input v-model="dSource" placeholder="来源" />
        <label>强度 (0-1)</label><input type="number" v-model.number="dStrength" min="0" max="1" step="0.1" />
        <div class="modal-actions"><button class="btn btn-outline" @click="showDelib = false">取消</button><button class="btn btn-primary" @click="addDelib">保存</button></div>
      </div>
    </div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const concerns = ref([])
const deliberations = ref([])
const showConcern = ref(false)
const showDelib = ref(false)
const cContent = ref(''); const cCategory = ref('social'); const cStrength = ref(0.5)
const dContent = ref(''); const dSource = ref('manual'); const dStrength = ref(0.5)
function fmtTime(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN') }
async function load() { const d = await api('/api/mental-state'); concerns.value = d.concerns || []; deliberations.value = d.deliberations || [] }
async function addConcern() { if (!cContent.value.trim()) return; await api('/api/mental-state/concerns', { method: 'POST', body: JSON.stringify({ content: cContent.value.trim(), category: cCategory.value, strength: cStrength.value }) }); showConcern.value = false; cContent.value = ''; load() }
async function delConcern(i) { if (!confirm('确定删除？')) return; await api('/api/mental-state/concerns/' + i, { method: 'DELETE' }); load() }
async function addDelib() { if (!dContent.value.trim()) return; await api('/api/mental-state/deliberations', { method: 'POST', body: JSON.stringify({ content: dContent.value.trim(), source: dSource.value || 'manual', strength: dStrength.value }) }); showDelib.value = false; dContent.value = ''; load() }
async function delDelib(i) { if (!confirm('确定删除？')) return; await api('/api/mental-state/deliberations/' + i, { method: 'DELETE' }); load() }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 20px 0 8px; color: var(--text-dim); }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: #fff; border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); margin-bottom: 16px; }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
tr:hover { background: var(--surface2); }
.truncate { max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; background: var(--purple-light); color: var(--purple); }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 20px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn-primary { background: linear-gradient(135deg, var(--accent), var(--purple)); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: #fff; border: 1.5px solid var(--border); color: var(--accent); }
.btn-sm { padding: 5px 12px; font-size: 11px; }
.modal-overlay { position: fixed; inset: 0; background: rgba(74,53,72,.4); backdrop-filter: blur(4px); display: flex; align-items: center; justify-content: center; z-index: 100; }
.modal { background: #fff; border: 2px solid var(--border); border-radius: 16px; padding: 28px; width: 500px; max-width: 90vw; box-shadow: 0 20px 60px rgba(236,72,153,.15); }
.modal h3 { margin-top: 0; font-size: 16px; color: var(--accent); }
.modal label { display: block; margin: 14px 0 4px; font-size: 12px; color: var(--text-dim); font-weight: 500; }
.modal input, .modal select, .modal textarea { width: 100%; background: var(--surface2); border: 1.5px solid var(--border); color: var(--text); padding: 10px 14px; border-radius: var(--radius); font-size: 13px; font-family: inherit; outline: none; }
.modal textarea { min-height: 80px; resize: vertical; }
.modal input:focus, .modal select:focus, .modal textarea:focus { border-color: var(--accent); }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 24px; }
</style>
