<template>
  <div>
    <div class="stat-row">
      <div class="stat-card">
        <div class="stat-num">{{ status.enabled ? '已启用' : '未启用' }}</div>
        <div class="stat-label">远程同步</div>
      </div>
      <div class="stat-card">
        <div class="stat-num" style="font-size:14px;word-break:break-all;line-height:1.4">{{ status.api_url || '-' }}</div>
        <div class="stat-label">API 地址</div>
      </div>
      <div class="stat-card">
        <div class="stat-num" style="font-size:18px">{{ status.db_name || '-' }}</div>
        <div class="stat-label">数据库名</div>
      </div>
    </div>

    <template v-if="status.enabled">
      <div class="section">
        <div class="toolbar" style="margin-bottom:0">
          <button class="btn btn-primary" :disabled="syncing" @click="doPush">{{ syncing === 'push' ? '推送中...' : '推送至远程' }}</button>
          <select v-model="pullMode" style="width:100px">
            <option value="merge">合并</option>
            <option value="replace">覆盖</option>
          </select>
          <button class="btn btn-success" :disabled="syncing" @click="doPull">{{ syncing === 'pull' ? '拉取中...' : '从远程拉取' }}</button>
        </div>
        <div v-if="syncResult" :class="['sync-msg', syncResult.ok ? 'ok' : 'err']">{{ syncResult.ok ? '✓' : '✗' }} {{ syncResult.msg }}</div>
      </div>

      <div class="section">
        <h3>已删除的记忆</h3>
        <div class="toolbar">
          <button class="btn btn-outline btn-sm" :disabled="loadingDeleted" @click="loadDeleted">{{ loadingDeleted ? '加载中...' : '加载' }}</button>
          <button class="btn btn-danger btn-sm" @click="purgeDeleted">清理过期</button>
        </div>
        <div v-if="!deletedLoaded" class="empty">点击"加载"查看</div>
        <div v-else-if="!deletedList.length" class="empty">没有已删除的记忆</div>
        <table v-else>
          <thead><tr><th>ID</th><th>内容</th><th>分类</th><th>操作</th></tr></thead>
          <tbody>
            <tr v-for="t in deletedList" :key="t.id">
              <td class="mono" style="font-size:11px">{{ (t.id||'').substring(0,8) }}...</td>
              <td class="truncate">{{ (t.content||'').length > 50 ? (t.content||'').substring(0,50)+'...' : t.content }}</td>
              <td><span class="badge badge-info">{{ t.category }}</span></td>
              <td class="actions">
                <button class="btn btn-success btn-sm" @click="restoreDeleted(t.id)">恢复</button>
                <button class="btn btn-danger btn-sm" @click="permDelete(t.id)">永久删除</button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
    <div v-else class="section">
      <p class="hint" style="text-align:center;padding:20px">请在 config.yaml 中配置 sync.enabled: true 和 sync.api_url 以启用同步功能</p>
    </div>
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
  if (pullMode.value === 'replace' && !confirm('覆盖模式将用远程数据完全替换本地数据，确定？')) return
  syncing.value = 'pull'; syncResult.value = null
  try { const d = await api('/api/sync/pull', { method: 'POST', body: JSON.stringify({ type: 'self_memory', mode: pullMode.value }) }); syncResult.value = { ok: true, msg: `拉取完成！新增 ${d.pulled} 条记忆` } }
  catch (e) { syncResult.value = { ok: false, msg: '拉取失败: ' + e.message } }
  finally { syncing.value = null }
}
async function loadDeleted() {
  loadingDeleted.value = true
  try { const d = await api('/api/sync/deleted'); deletedList.value = d.thoughts || []; deletedLoaded.value = true }
  catch { deletedList.value = []; deletedLoaded.value = true }
  finally { loadingDeleted.value = false }
}
async function restoreDeleted(id) { if (!confirm('确定恢复这条记忆？')) return; try { await api('/api/sync/restore', { method: 'POST', body: JSON.stringify({ id }) }); loadDeleted() } catch (e) { alert('恢复失败: ' + e.message) } }
async function permDelete(id) { if (!confirm('确定永久删除？此操作不可撤销！')) return; try { await api('/api/sync/remote', { method: 'DELETE', body: JSON.stringify({ id }) }); loadDeleted() } catch (e) { alert('删除失败: ' + e.message) } }
async function purgeDeleted() { if (!confirm('确定清理所有超过30天的已删除记忆？')) return; try { await api('/api/sync/purge', { method: 'POST' }); loadDeleted() } catch (e) { alert('清理失败: ' + e.message) } }
onMounted(load)
</script>

<style scoped>
.actions { display: flex; gap: 6px; }
.sync-msg { padding: 8px 14px; border-radius: 6px; font-size: 13px; margin-top: 12px; }
.sync-msg.ok { background: var(--success-bg); color: var(--success); }
.sync-msg.err { background: var(--danger-bg); color: var(--danger); }
.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
</style>
