<template>
  <div>
    <h2>🔄 数据同步</h2>
    <div class="stat-grid">
      <div class="stat-card"><div class="label">远程同步</div><div class="value" :style="{ color: status.enabled ? 'var(--success)' : 'var(--text-dim)' }">{{ status.enabled ? '✅ 已启用' : '❌ 未启用' }}</div></div>
      <div class="stat-card"><div class="label">API 地址</div><div class="value" style="font-size:11px;word-break:break-all">{{ status.api_url || '-' }}</div></div>
      <div class="stat-card"><div class="label">数据库名</div><div class="value" style="font-size:14px">{{ status.db_name || '-' }}</div></div>
    </div>
    <template v-if="status.enabled">
      <h3>🔄 自我记忆同步</h3>
      <div style="display:flex;gap:12px;margin-bottom:20px;flex-wrap:wrap">
        <button class="btn btn-primary" :disabled="syncing" @click="doPush">{{ syncing === 'push' ? '⏳ 推送中...' : '📤 Push (本地 → 远程)' }}</button>
        <div style="display:flex;gap:4px;align-items:center">
          <select v-model="pullMode" style="background:#fff;border:1.5px solid var(--border);color:var(--text);padding:8px 12px;border-radius:var(--radius);font-size:13px"><option value="merge">🔗 合并</option><option value="replace">♾️ 覆盖</option></select>
          <button class="btn btn-success" :disabled="syncing" @click="doPull">{{ syncing === 'pull' ? '⏳ 拉取中...' : '📥 Pull (远程 → 本地)' }}</button>
        </div>
      </div>
      <div v-if="syncResult" :class="['sync-result', syncResult.ok ? 'ok' : 'err']">
        {{ syncResult.ok ? '✅' : '❌' }} {{ syncResult.msg }}
      </div>
      <div class="deleted-section">
        <h3 style="color:var(--danger)">🗑 已删除的记忆 (远程)</h3>
        <div class="toolbar">
          <button class="btn btn-outline btn-sm" :disabled="loadingDeleted" @click="loadDeleted">{{ loadingDeleted ? '⏳ 加载中...' : '🔍 加载已删除记忆' }}</button>
          <button class="btn btn-danger btn-sm" @click="purgeDeleted">🗑 清理过期已删除</button>
        </div>
        <div v-if="!deletedLoaded" class="empty">点击"加载已删除记忆"查看</div>
        <div v-else-if="!deletedList.length" class="empty">😊 没有已删除的记忆</div>
        <table v-else><thead><tr><th>ID</th><th>内容</th><th>分类</th><th>操作</th></tr></thead><tbody>
          <tr v-for="t in deletedList" :key="t.id">
            <td class="mono" style="font-size:11px">{{ (t.id||'').substring(0,8) }}...</td>
            <td class="truncate">{{ (t.content||'').length > 50 ? (t.content||'').substring(0,50)+'...' : t.content }}</td>
            <td><span class="tag">{{ t.category }}</span></td>
            <td class="actions">
              <button class="btn btn-success btn-sm" @click="restoreDeleted(t.id)">恢复</button>
              <button class="btn btn-danger btn-sm" @click="permDelete(t.id)">永久删除</button>
            </td>
          </tr>
        </tbody></table>
      </div>
    </template>
    <div v-else class="empty" style="margin-top:40px">🔗 请在 config.yaml 中配置 sync.enabled: true 和 sync.api_url 以启用同步功能</div>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'
const status = ref({})
const syncing = ref(null)
const pullMode = ref('merge')
const syncResult = ref(null)
const deletedList = ref([])
const deletedLoaded = ref(false)
const loadingDeleted = ref(false)
async function load() { status.value = await api('/api/sync/status') }
async function doPush() {
  syncing.value = 'push'; syncResult.value = null
  try { const d = await api('/api/sync/push', { method: 'POST', body: JSON.stringify({ type: 'self_memory' }) }); syncResult.value = { ok: true, msg: `推送完成！已同步 ${d.synced} 条记忆到远程` } }
  catch (e) { syncResult.value = { ok: false, msg: '推送失败: ' + e.message } }
  finally { syncing.value = null }
}
async function doPull() {
  if (pullMode.value === 'replace' && !confirm('⚠️ 覆盖模式将用远程数据完全替换本地数据，确定？')) return
  syncing.value = 'pull'; syncResult.value = null
  try { const d = await api('/api/sync/pull', { method: 'POST', body: JSON.stringify({ type: 'self_memory', mode: pullMode.value }) }); syncResult.value = { ok: true, msg: `拉取完成 (${pullMode.value === 'replace' ? '覆盖' : '合并'})！新增 ${d.pulled} 条记忆` } }
  catch (e) { syncResult.value = { ok: false, msg: '拉取失败: ' + e.message } }
  finally { syncing.value = null }
}
async function loadDeleted() {
  loadingDeleted.value = true
  try { const d = await api('/api/sync/deleted'); deletedList.value = d.thoughts || []; deletedLoaded.value = true }
  catch (e) { deletedList.value = []; deletedLoaded.value = true }
  finally { loadingDeleted.value = false }
}
async function restoreDeleted(id) { if (!confirm('确定恢复这条记忆？')) return; try { await api('/api/sync/restore', { method: 'POST', body: JSON.stringify({ id }) }); loadDeleted() } catch (e) { alert('恢复失败: ' + e.message) } }
async function permDelete(id) { if (!confirm('⚠️ 确定永久删除？此操作不可撤销！')) return; try { await api('/api/sync/remote', { method: 'DELETE', body: JSON.stringify({ id }) }); loadDeleted() } catch (e) { alert('删除失败: ' + e.message) } }
async function purgeDeleted() { if (!confirm('确定清理所有超过30天的已删除记忆？')) return; try { await api('/api/sync/purge', { method: 'POST' }); loadDeleted() } catch (e) { alert('清理失败: ' + e.message) } }
onMounted(load)
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 20px 0 8px; color: var(--text-dim); font-weight: 500; }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; align-items: center; flex-wrap: wrap; }
.stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 12px; margin-bottom: 20px; }
.stat-card { background: var(--surface); border: 2px solid var(--accent-light); border-radius: var(--radius); padding: 16px; text-align: center; box-shadow: var(--shadow); }
.stat-card .label { font-size: 11px; color: var(--text-dim); margin-bottom: 4px; text-transform: uppercase; }
.stat-card .value { font-size: 24px; font-weight: 700; background: linear-gradient(135deg, var(--accent), var(--purple)); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }
.sync-result { padding: 12px 20px; border-radius: var(--radius); margin-bottom: 16px; font-size: 13px; }
.sync-result.ok { background: var(--success-light); color: var(--success); }
.sync-result.err { background: var(--danger-light); color: var(--danger); }
.deleted-section { margin-top: 24px; padding: 20px; background: var(--surface2); border: 2px solid var(--border); border-radius: var(--radius); }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
tr:hover { background: var(--surface2); }
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; background: var(--purple-light); color: var(--purple); }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.actions { display: flex; gap: 6px; }
.empty { text-align: center; padding: 20px; color: var(--text-dim); }
.btn { border: none; padding: 8px 16px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; display: inline-flex; align-items: center; gap: 4px; }
.btn:disabled { opacity: .6; cursor: not-allowed; }
.btn-primary { background: linear-gradient(135deg, var(--accent), var(--purple)); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-success { background: var(--success); color: #fff; }
.btn-outline { background: var(--surface); border: 1.5px solid var(--border); color: var(--accent); }
.btn-outline:hover { background: var(--accent-light); }
.btn-sm { padding: 5px 12px; font-size: 11px; }
</style>
