<template>
  <div>
    <h3>防注入安全系统</h3>

    <!-- 配置信息 -->
    <div class="section">
      <h3>⚙️ 防护配置</h3>
      <div class="config-grid">
        <div class="config-item">
          <span class="label">关键词过滤</span>
          <span class="value always-on">始终开启</span>
        </div>
        <div class="config-item">
          <span class="label">注入模式检测</span>
          <span class="value always-on">始终开启</span>
        </div>
        <div class="config-item">
          <span class="label">编码绕过检测</span>
          <span class="value always-on">始终开启</span>
        </div>
        <div class="config-item">
          <span class="label">色情/暴力/违法检测</span>
          <span class="value always-on">始终开启</span>
        </div>
        <div class="config-item">
          <span class="label">输出检测</span>
          <span class="value always-on">始终开启</span>
        </div>
        <div class="config-item">
          <span class="label">最大消息长度</span>
          <span class="value">{{ config.input?.max_message_length || 2000 }}</span>
        </div>
        <div class="config-item">
          <span class="label">敏感内容处理</span>
          <span class="value">{{ config.input?.sensitive_action || 'replace' }}</span>
        </div>
        <div class="config-item">
          <span class="label">输出处理</span>
          <span class="value">{{ config.output?.action || 'replace' }}</span>
        </div>
        <div class="config-item">
          <span class="label">频率限制</span>
          <span class="value">{{ config.behavior?.max_messages_per_minute || 20 }}/分钟</span>
        </div>
        <div class="config-item">
          <span class="label">信誉阈值</span>
          <span class="value">{{ config.behavior?.reputation_threshold || 0.3 }}</span>
        </div>
        <div class="config-item">
          <span class="label">自动封禁</span>
          <span class="value">{{ config.behavior?.auto_ban ? '已启用' : '已禁用' }}</span>
        </div>
      </div>
    </div>

    <!-- 用户风险总览 -->
    <div class="section">
      <h3>📊 用户风险总览
        <button class="btn btn-outline btn-sm" style="margin-left:8px" @click="loadUsers">刷新</button>
      </h3>
      <div v-if="allUsers.length === 0" class="empty">暂无用户记录</div>
      <table v-else class="risk-table">
        <thead>
          <tr>
            <th>QQ 号</th>
            <th>内容信誉</th>
            <th>信任信誉</th>
            <th>综合信誉</th>
            <th>违规</th>
            <th>高严重度</th>
            <th>惩罚系数</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="u in allUsers" :key="u.user_id"
              :class="getRiskClass(u.combined_reputation)"
              @click="expandUser(u.user_id)">
            <td class="uid">{{ u.user_id }}</td>
            <td>
              <div class="mini-bar">
                <div class="mini-fill" :class="getBarClass(u.content_reputation)" :style="{ width: u.content_reputation * 100 + '%' }"></div>
              </div>
              <span class="bar-text">{{ u.content_reputation?.toFixed(2) }}</span>
            </td>
            <td>
              <div class="mini-bar">
                <div class="mini-fill" :class="getBarClass(u.trust_reputation)" :style="{ width: u.trust_reputation * 100 + '%' }"></div>
              </div>
              <span class="bar-text">{{ u.trust_reputation?.toFixed(2) }}</span>
            </td>
            <td>
              <div class="mini-bar">
                <div class="mini-fill" :class="getBarClass(u.combined_reputation)" :style="{ width: u.combined_reputation * 100 + '%' }"></div>
              </div>
              <span class="bar-text">{{ u.combined_reputation?.toFixed(2) }}</span>
            </td>
            <td class="center">{{ u.violation_count || 0 }}</td>
            <td class="center" :class="{ 'text-danger': u.high_severity_count > 0 }">{{ u.high_severity_count || 0 }}</td>
            <td class="center" :class="{ 'text-warning': u.penalty_multiplier > 1 }">{{ u.penalty_multiplier?.toFixed(1) || '1.0' }}x</td>
            <td class="center">
              <span v-if="u.banned" class="badge badge-danger">封禁</span>
              <span v-else-if="u.silent_banned" class="badge badge-warning">静默封禁</span>
              <span v-else-if="u.vision_disabled" class="badge badge-info">识图禁用</span>
              <span v-else class="badge badge-ok">正常</span>
            </td>
            <td>
              <button class="btn btn-outline btn-xs" @click.stop="queryUid = String(u.user_id); queryUser()">详情</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- 查询用户详情 -->
    <div class="section">
      <h3>🔍 查询用户状态</h3>
      <div class="toolbar">
        <input v-model="queryUid" placeholder="输入 QQ 号" style="width:160px" @keydown.enter="queryUser" />
        <button class="btn btn-primary" @click="queryUser">查询</button>
      </div>
    </div>

    <!-- 用户状态详情 -->
    <div v-if="userStatus" class="section user-detail">
      <h3>👤 用户 {{ userStatus.user_id }} 详情</h3>
      <div class="status-grid">
        <div class="status-item" :class="getReputationClass(userStatus.reputation)">
          <span class="label">信誉分数</span>
          <span class="value">{{ userStatus.reputation?.toFixed(2) || '1.00' }}</span>
          <div class="progress-bar">
            <div class="progress-fill" :style="{ width: (userStatus.reputation * 100) + '%' }"></div>
          </div>
        </div>
        <div class="status-item" :class="{ 'danger': userStatus.violation_count > 0 }">
          <span class="label">违规次数</span>
          <span class="value">{{ userStatus.violation_count || 0 }}</span>
        </div>
        <div class="status-item" :class="{ 'danger': userStatus.vision_disabled }">
          <span class="label">识图状态</span>
          <span class="value">{{ userStatus.vision_disabled ? '❌ 已禁用' : '✅ 正常' }}</span>
        </div>
        <div class="status-item" :class="{ 'danger': userStatus.silent_banned }">
          <span class="label">静默封禁</span>
          <span class="value">{{ userStatus.silent_banned ? '⚠️ 已封禁' : '✅ 正常' }}</span>
        </div>
        <div class="status-item" :class="{ 'warning': userStatus.penalty_multiplier > 1 }">
          <span class="label">惩罚系数</span>
          <span class="value">{{ userStatus.penalty_multiplier?.toFixed(1) || '1.0' }}x</span>
        </div>
      </div>

      <div class="status-text" v-if="userStatus.status">
        <pre>{{ userStatus.status }}</pre>
      </div>

      <div class="actions">
        <button class="btn btn-success" @click="resetReputation" :disabled="!userStatus.user_id">🔄 重置信誉</button>
        <button class="btn btn-primary" @click="enableVision" :disabled="!userStatus.vision_disabled">👁️ 启用识图</button>
        <button class="btn btn-warning" @click="unbanUser" :disabled="!userStatus.silent_banned">🔓 解封用户</button>
        <button class="btn btn-danger" @click="banUser" :disabled="userStatus.silent_banned">🚫 完全封禁</button>
      </div>
    </div>

    <!-- 说明 -->
    <div class="section">
      <h3>📖 说明</h3>
      <ul class="help-list">
        <li><strong>信誉分数</strong>：0.0-1.0，低于 0.3 会触发非察觉性封禁</li>
        <li><strong>惩罚系数</strong>：信誉越低，AI 消耗的 token 越多（1.0x-6.0x）</li>
        <li><strong>静默封禁</strong>：用户会看到"使用人数过多"而非直接拒绝</li>
        <li><strong>识图禁用</strong>：违规用户的识图功能会被自动禁用</li>
        <li><strong>完全封禁</strong>：用户的所有消息将被完全忽略</li>
      </ul>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const config = ref({})
const queryUid = ref('')
const userStatus = ref(null)
const allUsers = ref([])

async function loadConfig() {
  try {
    const data = await api('/api/anti-injection')
    config.value = data.config || {}
  } catch (e) {
    console.error('Failed to load config:', e)
  }
}

async function loadUsers() {
  try {
    const data = await api('/api/anti-injection/users')
    const users = data.users || []
    // 按综合信誉升序（高风险在上）
    users.sort((a, b) => a.combined_reputation - b.combined_reputation)
    allUsers.value = users
  } catch (e) {
    console.error('Failed to load users:', e)
  }
}

function expandUser(uid) {
  queryUid.value = String(uid)
  queryUser()
}

async function queryUser() {
  const uid = parseInt(queryUid.value)
  if (!uid) return
  try {
    const data = await api('/api/anti-injection/' + uid)
    userStatus.value = data
  } catch (e) {
    alert('查询失败: ' + e.message)
  }
}

async function resetReputation() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定重置用户 ${userStatus.value.user_id} 的信誉？`)) return
  try {
    await api('/api/anti-injection/reset-reputation', { method: 'POST', body: JSON.stringify({ user_id: userStatus.value.user_id }) })
    queryUser()
    loadUsers()
  } catch (e) { alert('操作失败: ' + e.message) }
}

async function enableVision() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定为用户 ${userStatus.value.user_id} 启用识图？`)) return
  try {
    await api('/api/anti-injection/enable-vision', { method: 'POST', body: JSON.stringify({ user_id: userStatus.value.user_id }) })
    queryUser()
    loadUsers()
  } catch (e) { alert('操作失败: ' + e.message) }
}

async function unbanUser() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定解封用户 ${userStatus.value.user_id}？`)) return
  try {
    await api('/api/anti-injection/unban', { method: 'POST', body: JSON.stringify({ user_id: userStatus.value.user_id }) })
    queryUser()
    loadUsers()
  } catch (e) { alert('操作失败: ' + e.message) }
}

async function banUser() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定完全封禁用户 ${userStatus.value.user_id}？此操作不可逆！`)) return
  try {
    await api('/api/anti-injection/ban', { method: 'POST', body: JSON.stringify({ user_id: userStatus.value.user_id }) })
    queryUser()
    loadUsers()
  } catch (e) { alert('操作失败: ' + e.message) }
}

function getReputationClass(r) {
  if (r >= 0.8) return 'success'
  if (r >= 0.5) return 'warning'
  return 'danger'
}

function getRiskClass(r) {
  if (r >= 0.8) return 'row-ok'
  if (r >= 0.5) return 'row-warn'
  return 'row-danger'
}

function getBarClass(r) {
  if (r >= 0.8) return 'bar-ok'
  if (r >= 0.5) return 'bar-warn'
  return 'bar-danger'
}

onMounted(() => { loadConfig(); loadUsers() })
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin-bottom: 12px; font-weight: 600; color: var(--text); }

.section {
  background: var(--surface);
  border-radius: var(--radius);
  padding: 16px;
  margin-bottom: 16px;
  box-shadow: var(--shadow);
}

.config-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
}

.config-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: var(--accent-light);
  border-radius: var(--radius);
}

.config-item .label { font-size: 12px; color: var(--text-dim); }
.config-item .value { font-size: 13px; font-weight: 500; }
.always-on { color: var(--success); font-weight: 600; }

.toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
  align-items: center;
}

.toolbar input {
  background: var(--surface);
  border: 1.5px solid var(--border);
  color: var(--text);
  padding: 8px 12px;
  border-radius: var(--radius);
  font-size: 13px;
  outline: none;
}

.toolbar input:focus { border-color: var(--accent); }

.empty { color: var(--text-dim); font-size: 13px; padding: 12px 0; }

/* 风险总览表格 */
.risk-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.risk-table th {
  text-align: left;
  padding: 10px 8px;
  border-bottom: 2px solid var(--border);
  font-weight: 600;
  font-size: 12px;
  color: var(--text-dim);
  white-space: nowrap;
}

.risk-table td {
  padding: 8px;
  border-bottom: 1px solid var(--accent-light);
  vertical-align: middle;
}

.risk-table tr { cursor: pointer; transition: background .1s; }
.risk-table tr:hover { background: var(--accent-light); }
.row-ok { border-left: 3px solid var(--success); }
.row-warn { border-left: 3px solid var(--warning); }
.row-danger { border-left: 3px solid var(--danger); }

.uid { font-weight: 600; font-family: 'SFMono-Regular', Consolas, monospace; }
.center { text-align: center; }
.text-danger { color: var(--danger); font-weight: 600; }
.text-warning { color: var(--warning); font-weight: 600; }

.mini-bar {
  display: inline-block;
  width: 60px;
  height: 6px;
  background: var(--border);
  border-radius: 3px;
  overflow: hidden;
  vertical-align: middle;
  margin-right: 6px;
}

.mini-fill {
  height: 100%;
  border-radius: 3px;
  transition: width .3s;
}

.bar-ok { background: var(--success); }
.bar-warn { background: var(--warning); }
.bar-danger { background: var(--danger); }

.bar-text { font-size: 12px; font-family: 'SFMono-Regular', Consolas, monospace; }

.badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 11px;
  font-weight: 500;
}

.badge-ok { background: var(--success-light, #d1fae5); color: var(--success); }
.badge-info { background: var(--blue-light, #e0e7ff); color: var(--blue, #6366f1); }
.badge-warning { background: #fef3c7; color: #d97706; }
.badge-danger { background: var(--danger-light); color: var(--danger); }

/* 用户详情 */
.status-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}

.status-item {
  padding: 12px;
  background: var(--accent-light);
  border-radius: var(--radius);
  border-left: 3px solid var(--success);
}

.status-item.warning { border-left-color: var(--warning); }
.status-item.danger { border-left-color: var(--danger); }
.status-item .label { display: block; font-size: 11px; color: var(--text-dim); margin-bottom: 4px; }
.status-item .value { display: block; font-size: 16px; font-weight: 600; }

.progress-bar {
  height: 4px;
  background: var(--border);
  border-radius: 2px;
  margin-top: 8px;
  overflow: hidden;
}

.progress-fill { height: 100%; background: var(--success); transition: width .3s; }
.status-item.warning .progress-fill { background: var(--warning); }
.status-item.danger .progress-fill { background: var(--danger); }

.status-text {
  background: var(--accent-light);
  border-radius: var(--radius);
  padding: 12px;
  margin-bottom: 16px;
}

.status-text pre {
  font-family: 'SFMono-Regular', Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  margin: 0;
  white-space: pre-wrap;
}

.actions { display: flex; gap: 8px; flex-wrap: wrap; }

.btn {
  border: none;
  padding: 8px 16px;
  border-radius: var(--radius);
  cursor: pointer;
  font-size: 12px;
  font-weight: 500;
  transition: all .15s;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background: var(--accent); color: #fff; }
.btn-success { background: var(--success); color: #fff; }
.btn-warning { background: var(--warning); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }
.btn-outline { background: transparent; border: 1.5px solid var(--border); color: var(--text); }
.btn-sm { padding: 5px 12px; font-size: 11px; }
.btn-xs { padding: 3px 8px; font-size: 10px; }

.help-list {
  font-size: 13px;
  line-height: 1.8;
  padding-left: 20px;
}

.help-list li { margin-bottom: 4px; }
.help-list strong { color: var(--accent); }
</style>
