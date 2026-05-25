<template>
  <div>
    <div class="config-layout">
      <div class="config-nav glass-card">
        <div class="nav-group" v-for="group in sections" :key="group.id">
          <div class="nav-group-label">{{ group.label }}</div>
          <a v-for="sec in group.items" :key="sec.id"
             :class="{ active: activeSection === sec.id }"
             @click="activeSection = sec.id"
             class="nav-item">
            <span class="nav-dot" :style="{ background: sec.color }"></span>
            {{ sec.label }}
          </a>
        </div>
      </div>

      <div class="config-content">
        <div class="glass-card" v-for="sec in sections" :key="sec.id" v-show="activeSection === sec.id">
          <div class="card-header">
            <h3><span class="sec-dot" :style="{ background: sec.color }"></span> {{ sec.label }}</h3>
            <button class="btn btn-ghost btn-sm" @click="editSection = sec.id; editForm = {}; loadEditForm(sec)">✏️ 编辑</button>
          </div>
          <div v-if="!config" class="empty">加载中...</div>
          <div v-else class="field-list">
            <div v-for="f in sec.fields" :key="f.key" class="field-item">
              <div class="field-label">{{ f.label }}</div>
              <div class="field-value">
                <template v-if="f.type === 'bool'">
                  <span class="toggle-dot" :class="{ on: getVal(f) }"></span>
                  <span>{{ getVal(f) ? '是' : '否' }}</span>
                </template>
                <template v-else-if="f.type === 'array'">
                  <span class="array-chips">
                    <span v-for="(item, i) in (getVal(f) || [])" :key="i" class="chip-sm">{{ item }}</span>
                    <span v-if="!(getVal(f) || []).length" class="text-muted">空</span>
                  </span>
                </template>
                <template v-else>
                  <span class="mono">{{ getVal(f) ?? '-' }}</span>
                </template>
              </div>
            </div>
          </div>
        </div>

        <!-- Save Notice -->
        <div class="glass-card notice-card">
          <svg viewBox="0 0 20 20" fill="none" width="16" height="16"><path d="M10 2l7 3v5c0 4-3 7-7 8-4-1-7-4-7-8V5l7-3z" stroke="currentColor" stroke-width="1.5"/><path d="M9 9h2v5H9zM9 6h2v2H9z" fill="currentColor"/></svg>
          <span>修改配置后需点击「保存」并重启插件生效。编辑时只需填要改的字段，留空的字段保持原值。</span>
        </div>
      </div>
    </div>

    <!-- Edit Modal -->
    <div v-if="editSection" class="modal-overlay" @click.self="editSection = null">
      <div class="glass-card modal">
        <h3 style="margin-bottom:16px">编辑 {{ findSection(editSection)?.label }}</h3>
        <div class="edit-fields">
          <div v-for="f in findSection(editSection)?.fields || []" :key="f.key" class="edit-field">
            <label>{{ f.label }}</label>
            <input v-if="f.type === 'string' || f.type === 'number'"
                   :type="f.type === 'number' ? 'number' : 'text'"
                   v-model="editForm[f.key]" class="glass-input" :placeholder="f.default?.toString() || ''" />
            <select v-else-if="f.type === 'bool'" v-model="editForm[f.key]" class="glass-select">
              <option :value="true">是</option>
              <option :value="false">否</option>
            </select>
            <input v-else-if="f.type === 'array'"
                   v-model="editForm[f.key]" class="glass-input" placeholder="逗号分隔多个值" />
            <span v-else class="mono text-muted">{{ getVal(f) }}</span>
          </div>
        </div>
        <div class="modal-actions">
          <button class="btn btn-ghost" @click="editSection = null">取消</button>
          <button class="btn btn-primary" @click="saveConfig">保存</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, computed } from 'vue'
import { api } from '../api.js'

const config = ref(null)
const activeSection = ref('general')
const editSection = ref(null)
const editForm = reactive({})

const sections = [
  {
    id: 'general', label: '基础配置', color: '#6366f1',
    fields: [
      { key: 'api_key', label: 'API 密钥', type: 'string' },
      { key: 'base_url', label: 'API 地址', type: 'string' },
      { key: 'model', label: '模型名称', type: 'string' },
      { key: 'bot_name', label: 'Bot 名称', type: 'string' },
      { key: 'self_qq', label: 'Bot QQ 号', type: 'number' },
      { key: 'admin_qq', label: '管理员 QQ', type: 'number' },
      { key: 'darling_qq', label: 'Darling QQ', type: 'number' },
      { key: 'prompts', label: '人设文件', type: 'string' },
    ]
  },
  {
    id: 'ai', label: 'AI 参数', color: '#8b5cf6',
    fields: [
      { key: 'ai.frequency_penalty', label: '频率惩罚', type: 'number' },
      { key: 'ai.presence_penalty', label: '存在惩罚', type: 'number' },
      { key: 'ai.temperature', label: '温度', type: 'number' },
      { key: 'ai.top_p', label: 'Top P', type: 'number' },
      { key: 'ai.max_tokens', label: '最大 Tokens', type: 'number' },
      { key: 'ai.request_timeout', label: '请求超时(秒)', type: 'number' },
      { key: 'ai.analysis_max_tokens', label: '分析最大 Tokens', type: 'number' },
      { key: 'ai.analysis_temperature', label: '分析温度', type: 'number' },
    ]
  },
  {
    id: 'conversation', label: '对话', color: '#34d399',
    fields: [
      { key: 'conversation.max_history', label: '最大历史轮数', type: 'number' },
      { key: 'conversation.batch_timeout_ms', label: '批次超时(ms)', type: 'number' },
      { key: 'conversation.typing_speed', label: '打字速度', type: 'number' },
      { key: 'conversation.max_typing_delay_ms', label: '最大打字延迟(ms)', type: 'number' },
      { key: 'conversation.reply_follow_up_secs', label: '跟进回复间隔(秒)', type: 'number' },
      { key: 'conversation.intrusiveness_weight', label: '介入权重', type: 'number' },
      { key: 'conversation.action_descriptions', label: '动作描述', type: 'bool' },
      { key: 'conversation.reply_cooldown_secs', label: '回复冷却(秒)', type: 'number' },
    ]
  },
  {
    id: 'memory', label: '记忆', color: '#f472b6',
    fields: [
      { key: 'memory.normal_expire_days', label: '普通记忆过期(天)', type: 'number' },
      { key: 'memory.important_fade_days', label: '重要记忆衰减(天)', type: 'number' },
      { key: 'memory.auto_summarize_threshold', label: '自动摘要阈值', type: 'number' },
      { key: 'memory.working_memory_expire_hours', label: '工作记忆过期(小时)', type: 'number' },
    ]
  },
  {
    id: 'emotion', label: '情绪', color: '#f59e0b',
    fields: [
      { key: 'emotion.decay_rate', label: '衰减率', type: 'number' },
      { key: 'emotion.decay_delay_secs', label: '衰减延迟(秒)', type: 'number' },
      { key: 'emotion.neutral_threshold', label: '中性阈值', type: 'number' },
      { key: 'emotion.affinity_threshold', label: '好感阈值', type: 'number' },
    ]
  },
  {
    id: 'proactive', label: '主动对话', color: '#f97316',
    fields: [
      { key: 'proactive.enabled', label: '启用', type: 'bool' },
      { key: 'proactive.quiet_start', label: '免打扰开始(时)', type: 'number' },
      { key: 'proactive.quiet_end', label: '免打扰结束(时)', type: 'number' },
      { key: 'proactive.interval', label: '间隔(秒)', type: 'number' },
      { key: 'proactive.max_ignore', label: '最大忽略次数', type: 'number' },
      { key: 'proactive.low_mood_multiplier', label: '低情绪倍率', type: 'number' },
      { key: 'proactive.check_interval', label: '检查间隔(秒)', type: 'number' },
    ]
  },
  {
    id: 'reflection', label: '自我反思', color: '#60a5fa',
    fields: [
      { key: 'self_reflection.interval', label: '反思间隔(秒)', type: 'number' },
      { key: 'self_reflection.max_thoughts', label: '最大想法数', type: 'number' },
      { key: 'self_reflection.post_conversation_delay_secs', label: '对话后延迟(秒)', type: 'number' },
    ]
  },
  {
    id: 'mental', label: '心理状态', color: '#a78bfa',
    fields: [
      { key: 'mental_state.concerns_max', label: '最大担忧数', type: 'number' },
      { key: 'mental_state.concern_decay_rate', label: '担忧衰减率', type: 'number' },
      { key: 'mental_state.deliberations_max', label: '最大考量数', type: 'number' },
      { key: 'mental_state.deliberation_decay_rate', label: '考量衰减率', type: 'number' },
      { key: 'mental_state.defect_base_probability', label: '缺陷基础概率', type: 'number' },
    ]
  },
  {
    id: 'vision', label: '识图', color: '#06b6d4',
    fields: [
      { key: 'vision.api_key', label: 'API 密钥', type: 'string' },
      { key: 'vision.base_url', label: 'API 地址', type: 'string' },
      { key: 'vision.model', label: '模型', type: 'string' },
      { key: 'vision.max_tokens', label: '最大 Tokens', type: 'number' },
    ]
  },
  {
    id: 'embedding', label: '向量嵌入', color: '#14b8a6',
    fields: [
      { key: 'embedding.api_key', label: 'API 密钥', type: 'string' },
      { key: 'embedding.base_url', label: 'API 地址', type: 'string' },
      { key: 'embedding.model', label: '模型', type: 'string' },
    ]
  },
  {
    id: 'style', label: '回复风格', color: '#e879f9',
    fields: [
      { key: 'style.omit_subject', label: '省略主语', type: 'bool' },
      { key: 'style.punctuation_style', label: '标点风格', type: 'string' },
      { key: 'style.max_reply_chars', label: '最大回复字数', type: 'number' },
    ]
  },
  {
    id: 'access', label: '访问控制', color: '#fb923c',
    fields: [
      { key: 'whitelist', label: '白名单', type: 'array' },
      { key: 'blacklist', label: '黑名单', type: 'array' },
      { key: 'auto_start_users', label: '自动启动用户', type: 'array' },
      { key: 'auto_start_groups', label: '自动启动群', type: 'array' },
    ]
  },
  {
    id: 'anti_injection', label: '防注入', color: '#ef4444',
    fields: [
      { key: 'anti_injection.input.max_message_length', label: '最大消息长度', type: 'number' },
      { key: 'anti_injection.input.sensitive_action', label: '敏感操作', type: 'string' },
      { key: 'anti_injection.output.action', label: '输出处理', type: 'string' },
      { key: 'anti_injection.behavior.rate_limit', label: '速率限制', type: 'bool' },
      { key: 'anti_injection.behavior.max_messages_per_minute', label: '每分钟上限', type: 'number' },
      { key: 'anti_injection.behavior.max_messages_per_hour', label: '每小时上限', type: 'number' },
      { key: 'anti_injection.behavior.reputation_threshold', label: '信誉阈值', type: 'number' },
      { key: 'anti_injection.behavior.auto_ban', label: '自动封禁', type: 'bool' },
      { key: 'anti_injection.behavior.auto_ban_threshold', label: '封禁触发次数', type: 'number' },
    ]
  },
  {
    id: 'quota', label: '配额', color: '#a3e635',
    fields: [
      { key: 'quota.enabled', label: '启用', type: 'bool' },
      { key: 'quota.max_per_minute', label: '每分钟上限', type: 'number' },
      { key: 'quota.max_per_hour', label: '每小时上限', type: 'number' },
      { key: 'quota.max_per_day', label: '每天上限', type: 'number' },
    ]
  },
  {
    id: 'sync', label: '远程同步', color: '#67e8f9',
    fields: [
      { key: 'sync.enabled', label: '启用', type: 'bool' },
      { key: 'sync.server_url', label: '服务器地址', type: 'string' },
      { key: 'sync.api_key', label: 'API 密钥', type: 'string' },
      { key: 'sync.sync_interval', label: '同步间隔(秒)', type: 'number' },
    ]
  },
  {
    id: 'sticker', label: '表情包', color: '#f0abfc',
    fields: [
      { key: 'sticker.auto_reply_probability', label: '自动回复概率', type: 'number' },
    ]
  },
]

function getVal(f) {
  const key = f.key
  if (!config.value) return null
  const parts = key.split('.')
  let v = config.value
  for (const p of parts) {
    if (v == null || typeof v !== 'object') return null
    v = v[p]
  }
  return v
}

function findSection(id) { return sections.find(s => s.id === id) }

function loadEditForm(sec) {
  for (const f of sec.fields) {
    const v = getVal(f)
    if (v != null) editForm[f.key] = f.type === 'array' ? (v || []).join(', ') : v
    else editForm[f.key] = ''
  }
}

async function load() {
  try { config.value = await api('/api/config') } catch {}
}

async function saveConfig() {
  if (!editSection.value) return
  const sec = findSection(editSection.value)
  if (!sec) return

  // Build a deep merge object from editForm
  const patch = {}
  for (const f of sec.fields) {
    if (editForm[f.key] === '' || editForm[f.key] === undefined) continue
    const parts = f.key.split('.')
    let current = patch
    for (let i = 0; i < parts.length; i++) {
      if (i === parts.length - 1) {
        if (f.type === 'number') current[parts[i]] = Number(editForm[f.key])
        else if (f.type === 'bool') current[parts[i]] = editForm[f.key] === true || editForm[f.key] === 'true'
        else if (f.type === 'array') current[parts[i]] = String(editForm[f.key]).split(',').map(s => s.trim()).filter(Boolean)
        else current[parts[i]] = editForm[f.key]
      } else {
        current[parts[i]] = current[parts[i]] || {}
        current = current[parts[i]]
      }
    }
  }

  try {
    await api('/api/config', { method: 'PUT', body: JSON.stringify(patch) })
    editSection.value = null
    load()
  } catch (e) {
    alert('保存失败: ' + e.message)
  }
}

onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.config-layout { display: flex; gap: 20px; align-items: flex-start; }
.config-nav { width: 200px; padding: 16px; position: sticky; top: 84px; flex-shrink: 0; }
.nav-group { margin-bottom: 16px; }
.nav-group-label { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 1px; color: var(--text-3); padding: 0 4px; margin-bottom: 4px; }
.nav-item { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: var(--radius-xs); font-size: 12px; font-weight: 500; cursor: pointer; transition: var(--transition); color: var(--text-2); }
.nav-item:hover { background: var(--surface-hover); color: var(--text); }
.nav-item.active { background: var(--primary); color: white; }
.nav-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
.config-content { flex: 1; min-width: 0; }
.glass-card { padding: 20px; border-radius: var(--radius); backdrop-filter: blur(16px) saturate(1.5); -webkit-backdrop-filter: blur(16px) saturate(1.5); background: var(--surface); border: 1px solid var(--glass-border); box-shadow: var(--glass-shadow); margin-bottom: 16px; }
.card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.card-header h3 { font-size: 15px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
.sec-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
.btn { padding: 8px 14px; border: none; border-radius: var(--radius-xs); font-size: 13px; font-weight: 500; cursor: pointer; transition: var(--transition); }
.btn-primary { background: var(--primary); color: white; }
.btn-ghost { background: var(--surface); color: var(--text); border: 1px solid var(--glass-border); }
.btn-sm { padding: 4px 10px; font-size: 12px; }
.empty { text-align: center; padding: 24px; color: var(--text-3); }
.field-list { display: flex; flex-direction: column; }
.field-item { display: flex; align-items: center; padding: 8px 0; border-bottom: 1px solid var(--glass-border); font-size: 13px; }
.field-item:last-child { border-bottom: none; }
.field-label { width: 160px; color: var(--text-2); font-weight: 500; flex-shrink: 0; }
.field-value { flex: 1; display: flex; align-items: center; gap: 6px; }
.mono { font-family: monospace; font-size: 12px; color: var(--text); }
.text-muted { color: var(--text-3); }
.toggle-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-3); }
.toggle-dot.on { background: var(--success); }
.array-chips { display: flex; flex-wrap: wrap; gap: 4px; }
.chip-sm { font-size: 11px; padding: 2px 8px; background: var(--surface-hover); border-radius: 4px; color: var(--text-2); }
.notice-card { display: flex; align-items: center; gap: 10px; font-size: 12px; color: var(--text-2); line-height: 1.5; }
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); z-index: 200; display: flex; align-items: center; justify-content: center; }
.modal { width: 480px; max-height: 80vh; overflow-y: auto; padding: 24px; }
.modal h3 { margin-bottom: 16px; }
.edit-fields { display: flex; flex-direction: column; gap: 12px; }
.edit-field { display: flex; flex-direction: column; gap: 4px; }
.edit-field label { font-size: 12px; font-weight: 600; color: var(--text-2); }
.glass-input, .glass-select { padding: 8px 12px; border-radius: var(--radius-xs); border: 1px solid var(--glass-border); background: var(--surface); color: var(--text); font-size: 13px; outline: none; width: 100%; }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }

@media (max-width: 768px) {
  .config-layout { flex-direction: column; }
  .config-nav { width: 100%; position: static; display: flex; flex-wrap: wrap; gap: 4px; }
  .nav-group { margin-bottom: 8px; min-width: 100%; }
  .nav-group .nav-item { display: inline-flex; margin: 2px; }
}
</style>