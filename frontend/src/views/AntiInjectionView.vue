<template>
  <div>
    <h2>🛡️ 防注入安全系统</h2>

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

    <!-- 查询用户 -->
    <div class="section">
      <h3>🔍 查询用户状态</h3>
      <div class="toolbar">
        <input v-model="queryUid" placeholder="输入 QQ 号" style="width:160px" @keydown.enter="queryUser" />
        <button class="btn btn-primary" @click="queryUser">查询</button>
      </div>
    </div>

    <!-- 用户状态详情 -->
    <div v-if="userStatus" class="section user-detail">
      <h3>👤 用户 {{ userStatus.user_id }} 状态</h3>
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

      <!-- 状态文本 -->
      <div class="status-text" v-if="userStatus.status">
        <pre>{{ userStatus.status }}</pre>
      </div>

      <!-- 操作按钮 -->
      <div class="actions">
        <button class="btn btn-success" @click="resetReputation" :disabled="!userStatus.user_id">
          🔄 重置信誉
        </button>
        <button class="btn btn-primary" @click="enableVision" :disabled="!userStatus.vision_disabled">
          👁️ 启用识图
        </button>
        <button class="btn btn-warning" @click="unbanUser" :disabled="!userStatus.silent_banned">
          🔓 解封用户
        </button>
        <button class="btn btn-danger" @click="banUser" :disabled="userStatus.silent_banned">
          🚫 完全封禁
        </button>
      </div>
    </div>

    <!-- 说明 -->
    <div class="section">
      <h3>📖 说明</h3>
      <ul class="help-list">
        <li><strong>信誉分数</strong>：0.0-1.0，低于 0.3 会触发非察觉性封禁</li>
        <li><strong>惩罚系数</strong>：信誉越低，AI 消耗的 token 越多（1.0x-4.0x）</li>
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

async function loadConfig() {
  try {
    const data = await api('/api/anti-injection')
    config.value = data.config || {}
  } catch (e) {
    console.error('Failed to load config:', e)
  }
}

async function queryUser() {
  const uid = parseInt(queryUid.value)
  if (!uid) return

  try {
    const data = await api('/api/anti-injection/' + uid)
    userStatus.value = data
  } catch (e) {
    console.error('Failed to query user:', e)
    alert('查询失败: ' + e.message)
  }
}

async function resetReputation() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定重置用户 ${userStatus.value.user_id} 的信誉？`)) return

  try {
    await api('/api/anti-injection/reset-reputation', {
      method: 'POST',
      body: JSON.stringify({ user_id: userStatus.value.user_id })
    })
    queryUser() // 刷新状态
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

async function enableVision() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定为用户 ${userStatus.value.user_id} 启用识图？`)) return

  try {
    await api('/api/anti-injection/enable-vision', {
      method: 'POST',
      body: JSON.stringify({ user_id: userStatus.value.user_id })
    })
    queryUser() // 刷新状态
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

async function unbanUser() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定解封用户 ${userStatus.value.user_id}？`)) return

  try {
    await api('/api/anti-injection/unban', {
      method: 'POST',
      body: JSON.stringify({ user_id: userStatus.value.user_id })
    })
    queryUser() // 刷新状态
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

async function banUser() {
  if (!userStatus.value?.user_id) return
  if (!confirm(`确定完全封禁用户 ${userStatus.value.user_id}？此操作不可逆！`)) return

  try {
    await api('/api/anti-injection/ban', {
      method: 'POST',
      body: JSON.stringify({ user_id: userStatus.value.user_id })
    })
    queryUser() // 刷新状态
  } catch (e) {
    alert('操作失败: ' + e.message)
  }
}

function getReputationClass(reputation) {
  if (reputation >= 0.8) return 'success'
  if (reputation >= 0.5) return 'warning'
  return 'danger'
}

onMounted(loadConfig)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin-bottom: 12px; font-weight: 600; color: var(--text); }

.section {
  background: #fff;
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

.config-item .label {
  font-size: 12px;
  color: var(--text-dim);
}

.config-item .value {
  font-size: 13px;
  font-weight: 500;
}

.always-on {
  color: var(--success);
  font-weight: 600;
}

.enabled {
  color: var(--success);
}

.toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
  align-items: center;
}

.toolbar input {
  background: #fff;
  border: 1.5px solid var(--border);
  color: var(--text);
  padding: 8px 12px;
  border-radius: var(--radius);
  font-size: 13px;
  outline: none;
}

.toolbar input:focus {
  border-color: var(--accent);
}

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

.status-item.warning {
  border-left-color: var(--warning);
}

.status-item.danger {
  border-left-color: var(--danger);
}

.status-item .label {
  display: block;
  font-size: 11px;
  color: var(--text-dim);
  margin-bottom: 4px;
}

.status-item .value {
  display: block;
  font-size: 16px;
  font-weight: 600;
}

.progress-bar {
  height: 4px;
  background: var(--border);
  border-radius: 2px;
  margin-top: 8px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--success);
  transition: width 0.3s ease;
}

.status-item.warning .progress-fill {
  background: var(--warning);
}

.status-item.danger .progress-fill {
  background: var(--danger);
}

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

.actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

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

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-primary { background: var(--accent); color: #fff; }
.btn-success { background: var(--success); color: #fff; }
.btn-warning { background: var(--warning); color: #fff; }
.btn-danger { background: var(--danger); color: #fff; }

.help-list {
  font-size: 13px;
  line-height: 1.8;
  padding-left: 20px;
}

.help-list li {
  margin-bottom: 4px;
}

.help-list strong {
  color: var(--accent);
}
</style>
