<template>
  <div>
    <div class="top-bar">
      <h2>⚙️ 配置管理</h2>
      <div class="top-actions">
        <span v-if="saveMsg" :class="saveOk ? 'save-ok' : 'save-err'">{{ saveMsg }}</span>
        <button class="btn btn-outline" @click="load" :disabled="loading">🔄 重新加载</button>
        <button class="btn btn-primary" @click="save" :disabled="saving">
          {{ saving ? '⏳ 保存中...' : '💾 保存配置' }}
        </button>
      </div>
    </div>

    <!-- 关键配置（始终显示） -->
    <div class="section highlight">
      <h3>🔑 基础配置</h3>
      <div class="field-grid">
        <div class="field">
          <label title="OpenAI 兼容接口的 API Key，支持 DeepSeek / OpenAI / 通义千问 / 硅基流动等">API Key</label>
          <input v-model="cfg.api_key" type="password" placeholder="sk-..." />
          <span class="hint">已脱敏，保存时保留原值</span>
        </div>
        <div class="field">
          <label title="OpenAI 兼容接口地址，如 https://api.deepseek.com/v1">Base URL</label>
          <input v-model="cfg.base_url" placeholder="https://api.deepseek.com/v1" />
        </div>
        <div class="field">
          <label title="使用的模型名称，如 deepseek-chat、gpt-4o 等">模型</label>
          <input v-model="cfg.model" placeholder="deepseek-chat" />
        </div>
        <div class="field">
          <label title="机器人自身 QQ 号，用于判断群消息是否 @了机器人。设为 0 则回复所有消息">Bot QQ 号</label>
          <input v-model.number="cfg.self_qq" type="number" />
          <span class="hint">0 = 回复所有消息</span>
        </div>
        <div class="field">
          <label title="管理员 QQ 号，只有此人可以使用控制命令。设为 0 则所有人可用">管理员 QQ</label>
          <input v-model.number="cfg.admin_qq" type="number" />
          <span class="hint">0 = 所有人都是管理员</span>
        </div>
        <div class="field">
          <label title="放在 prompts/ 目录下的文件名，定义 AI 的身份和人设">提示词文件</label>
          <input v-model="cfg.prompts" placeholder="default.txt" />
        </div>
      </div>
    </div>

    <!-- 白名单/黑名单（始终显示） -->
    <div class="section highlight">
      <h3>🔒 用户访问控制</h3>
      <p class="desc">白名单优先：配置白名单后只允许白名单用户使用私聊，黑名单无效。两个都为空则全体可用。</p>
      <div class="field-grid cols-3">
        <div class="field">
          <label title="只允许这些用户使用私聊，为空则不限制。白名单优先于黑名单">白名单 (每行一个 QQ 号)</label>
          <textarea v-model="whitelistText" rows="3" placeholder="留空 = 不限制，所有用户可用"></textarea>
        </div>
        <div class="field">
          <label title="禁止这些用户使用私聊，为空则不限制">黑名单 (每行一个 QQ 号)</label>
          <textarea v-model="blacklistText" rows="3" placeholder="留空 = 不限制"></textarea>
        </div>
        <div class="field">
          <label title="启动插件后自动开启这些用户的私聊，无需手动发送开启对话">自动开启私聊 (每行一个 QQ 号)</label>
          <textarea v-model="autoStartText" rows="3" placeholder="启动后自动开启这些用户的私聊"></textarea>
        </div>
      </div>
    </div>

    <!-- 可折叠配置区 -->
    <div class="section" v-for="s in sections" :key="s.key">
      <h3 class="collapsible" @click="toggle(s.key)">
        <span>{{ expanded[s.key] ? '▾' : '▸' }} {{ s.icon }} {{ s.title }}</span>
      </h3>
      <div v-show="expanded[s.key]" class="section-body">
        <div class="field-grid">
          <div class="field" v-for="f in s.fields" :key="f.key" :title="f.tip">
            <label>
              {{ f.label }}
              <span v-if="f.type === 'range'" class="range-val">{{ getFieldVal(f) ?? f.default }}</span>
            </label>
            <select v-if="f.type === 'select'" :value="getFieldVal(f)" @input="setFieldVal(f, $event.target.value === 'true' ? true : $event.target.value === 'false' ? false : $event.target.value)">
              <option v-for="o in f.options" :key="o.value" :value="o.value">{{ o.label }}</option>
            </select>
            <input v-else-if="f.type === 'range'" type="range" :min="f.min" :max="f.max" :step="f.step"
                   :value="getFieldVal(f)" @input="setFieldVal(f, parseFloat($event.target.value))" />
            <input v-else-if="f.type === 'number'" type="number" :step="f.step || 1"
                   :value="getFieldVal(f)" @input="setFieldVal(f, parseFloat($event.target.value))" />
            <input v-else type="text" :value="getFieldVal(f)" @input="setFieldVal(f, $event.target.value)" />
            <span v-if="f.hint" class="hint">{{ f.hint }}</span>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const DEFAULTS = {
  api_key: '', base_url: '', model: '', self_qq: 0, admin_qq: 0, prompts: 'default.txt',
  ai: { frequency_penalty: 2.0, presence_penalty: 1.0, temperature: 1.3, top_p: 0.1, max_tokens: 4096, request_timeout: 60, analysis_max_tokens: 10000, analysis_temperature: 0.3 },
  conversation: { max_history: 10, batch_timeout_ms: 6000, typing_speed: 5.0, max_typing_delay_ms: 4000, reply_follow_up_secs: 300, action_descriptions: false },
  memory: { normal_expire_days: 30, important_fade_days: 7, auto_summarize_threshold: 10, working_memory_expire_hours: 6 },
  emotion: { decay_rate: 0.15, decay_delay_secs: 60, neutral_threshold: 0.15, affinity_threshold: 3.0 },
  proactive: { enabled: true, quiet_start: 23, quiet_end: 7, interval: 7200, max_ignore: 3, low_mood_multiplier: 2.0 },
  self_reflection: { interval: 1800, max_thoughts: 8, post_conversation_delay_secs: 120 },
  mental_state: { concerns_max: 5, concern_decay_rate: 0.1, deliberations_max: 8, deliberation_decay_rate: 0.05, defect_base_probability: 0.1 },
  style: { max_reply_chars: 30, omit_subject: true, punctuation_style: 'casual' },
  log: { enabled: true, level: 'info' },
  vision: { api_key: '', base_url: '', model: '', max_tokens: 256 },
  whitelist: [], blacklist: [], auto_start_users: [],
}

const sections = [
  { key: 'ai', icon: '🤖', title: 'AI 参数', fields: [
    { key: 'ai.frequency_penalty', label: '频率惩罚', type: 'range', min: -2, max: 2, step: 0.1, default: 2.0,
      tip: '降低模型重复用词的概率 (-2.0~2.0)。越高越不容易重复，建议 1.5~2.5', hint: '建议 1.5~2.5' },
    { key: 'ai.presence_penalty', label: '存在惩罚', type: 'range', min: -2, max: 2, step: 0.1, default: 1.0,
      tip: '促进模型谈论新话题 (-2.0~2.0)。越高越倾向于引入新内容，建议 0.8~1.5', hint: '建议 0.8~1.5' },
    { key: 'ai.temperature', label: '温度', type: 'range', min: 0, max: 2, step: 0.1, default: 1.3,
      tip: '控制回复的随机性 (0.0~2.0)。越高越随机有创意，越低越确定保守。日常聊天建议 1.0~1.5', hint: '日常 1.0~1.5' },
    { key: 'ai.top_p', label: 'Top P', type: 'range', min: 0, max: 1, step: 0.05, default: 0.1,
      tip: '核采样，控制候选词范围 (0.0~1.0)。越低回复越集中，越高越多样。建议 0.1~0.3', hint: '建议 0.1~0.3' },
    { key: 'ai.max_tokens', label: '最大 Token', type: 'number',
      tip: '最大回复 token 数。日常聊天建议 1024~2048，长文生成建议 4096', hint: '日常 1024~2048' },
    { key: 'ai.request_timeout', label: '请求超时 (秒)', type: 'number',
      tip: 'API 请求超时时间（秒）。网络慢时可适当增大' },
    { key: 'ai.analysis_max_tokens', label: '分析任务 Token', type: 'number',
      tip: '分析任务（记忆提取、情绪分析、自我反思等）的最大 token 数' },
    { key: 'ai.analysis_temperature', label: '分析温度', type: 'range', min: 0, max: 1, step: 0.05, default: 0.3,
      tip: '分析任务的温度。越低越确定，建议 0.1~0.5', hint: '建议 0.1~0.5' },
  ]},
  { key: 'conversation', icon: '💬', title: '对话参数', fields: [
    { key: 'conversation.max_history', label: '历史轮数', type: 'number',
      tip: '对话历史保留轮数 (每轮=1条用户消息+1条AI回复)。越大上下文越长，API 费用越高。建议 5~15', hint: '建议 5~15' },
    { key: 'conversation.batch_timeout_ms', label: '合并超时 (ms)', type: 'number',
      tip: '用户短时间内发送的连续消息会合并为一条发给 AI。避免分段发送触发多次 API 调用。建议 4000~8000', hint: '建议 4000~8000' },
    { key: 'conversation.typing_speed', label: '打字速度 (字/秒)', type: 'number', step: 0.5,
      tip: 'AI 回复按 |^| 分割为多条消息，每条之间根据长度计算延迟。越快越像秒回，越慢越像真人在打字。建议 3.0~8.0', hint: '建议 3.0~8.0' },
    { key: 'conversation.max_typing_delay_ms', label: '最大打字延迟 (ms)', type: 'number',
      tip: '单条消息的最大等待时间，防止长消息等待过久' },
    { key: 'conversation.reply_follow_up_secs', label: '跟进超时 (秒)', type: 'number',
      tip: '用户 @机器人 发起对话后，在此时间内后续消息即使没 @也会回复。超过后必须 @才会回复。建议 120~600', hint: '建议 120~600' },
    { key: 'conversation.action_descriptions', label: '动作描述', type: 'select', options: [{value:true,label:'允许'},{value:false,label:'禁止'}],
      tip: '是否允许 AI 用括号描述动作和表情，如"（笑了笑）"、"（叹气）"' },
  ]},
  { key: 'memory', icon: '🧠', title: '记忆参数', fields: [
    { key: 'memory.normal_expire_days', label: '普通记忆过期 (天)', type: 'number',
      tip: '超过此天数未被访问的普通记忆会被淡忘（不再注入 prompt）。设为 0 则永不过期', hint: '0 = 永不过期' },
    { key: 'memory.important_fade_days', label: '重要记忆降权 (天)', type: 'number',
      tip: '超过此天数未被访问的重要记忆会被标记为 [淡忘]' },
    { key: 'memory.auto_summarize_threshold', label: '自动摘要阈值', type: 'number',
      tip: '对话历史超过此轮数时，自动提取摘要存为普通记忆' },
    { key: 'memory.working_memory_expire_hours', label: '工作记忆过期 (小时)', type: 'number',
      tip: '所有群聊消息都会临时存储，超过此时间后自动清理' },
  ]},
  { key: 'emotion', icon: '😊', title: '情绪参数', fields: [
    { key: 'emotion.decay_rate', label: '衰减速率', type: 'range', min: 0.01, max: 0.5, step: 0.01, default: 0.15,
      tip: '情绪衰减速率 (每小时衰减的强度值)。越高恢复平静越快。0.1=约10小时恢复，0.3=约3小时', hint: '0.1=慢恢复, 0.3=快恢复' },
    { key: 'emotion.decay_delay_secs', label: '衰减延迟 (秒)', type: 'number',
      tip: '此时间内情绪不衰减，保持刚变化后的状态' },
    { key: 'emotion.neutral_threshold', label: '平静阈值', type: 'range', min: 0.05, max: 0.5, step: 0.05, default: 0.15,
      tip: '情绪强度低于此值时自动恢复为 Neutral（平静）' },
    { key: 'emotion.affinity_threshold', label: '亲近感阈值 (次/时)', type: 'number', step: 0.5,
      tip: '互动频率超过此值时，AI 会表现得更亲近' },
  ]},
  { key: 'proactive', icon: '📢', title: '主动对话', fields: [
    { key: 'proactive.enabled', label: '启用', type: 'select', options: [{value:true,label:'开启'},{value:false,label:'关闭'}],
      tip: '是否开启主动对话。开启后 AI 会根据情绪和时间主动找用户聊天' },
    { key: 'proactive.quiet_start', label: '免打扰开始 (时)', type: 'number',
      tip: '免打扰时段开始 (24小时制)。支持跨午夜：如 23 表示晚上11点' },
    { key: 'proactive.quiet_end', label: '免打扰结束 (时)', type: 'number',
      tip: '免打扰时段结束 (24小时制)。如 7 表示早上7点' },
    { key: 'proactive.interval', label: '主动间隔 (秒)', type: 'number',
      tip: '用户超过此时间未对话，且满足其他条件时发送问候。7200=2小时', hint: '7200 = 2小时' },
    { key: 'proactive.max_ignore', label: '最大忽略次数', type: 'number',
      tip: '用户忽略主动消息达到此次数后，降低发送频率' },
    { key: 'proactive.low_mood_multiplier', label: '低情绪倍率', type: 'number', step: 0.1,
      tip: 'AI 情绪低落 (sad/tired) 时，主动消息间隔乘以此系数' },
  ]},
  { key: 'self_reflection', icon: '🪞', title: '自我反思', fields: [
    { key: 'self_reflection.interval', label: '反思间隔 (秒)', type: 'number',
      tip: '每隔此时间，bot 会回顾最近的对话并产生内心想法。1800=30分钟，建议 900~3600', hint: '1800 = 30分钟' },
    { key: 'self_reflection.max_thoughts', label: '注入想法数', type: 'number',
      tip: '每次对话时注入多少条最近的自我想法作为上下文。所有想法都会永久保存。建议 5~15', hint: '建议 5~15' },
    { key: 'self_reflection.post_conversation_delay_secs', label: '对话后延迟 (秒)', type: 'number',
      tip: '对话结束后多久触发反思。120=2分钟，建议 60~300', hint: '120 = 2分钟' },
  ]},
  { key: 'mental_state', icon: '🧬', title: '心理状态', fields: [
    { key: 'mental_state.concerns_max', label: '最大担忧数', type: 'number',
      tip: '最大活跃担忧数。超出上限时最弱的担忧会被替换' },
    { key: 'mental_state.concern_decay_rate', label: '担忧衰减 (每小时)', type: 'number', step: 0.01,
      tip: '担忧衰减速率。越高消退越快，0.1=约10小时从满强度消退' },
    { key: 'mental_state.deliberations_max', label: '最大考量数', type: 'number',
      tip: '从对话中积累的行事准则和人际洞察的上限' },
    { key: 'mental_state.deliberation_decay_rate', label: '考量衰减 (每小时)', type: 'number', step: 0.01,
      tip: '考量衰减速率。比担忧衰减更慢，因为考量代表积累的智慧' },
    { key: 'mental_state.defect_base_probability', label: '缺陷概率', type: 'range', min: 0, max: 0.5, step: 0.01, default: 0.1,
      tip: '缺陷基础触发概率 (0.0~1.0)。实际概率受情绪影响：疲惫/愤怒时更高，开心/平静时更低。0.0=禁用', hint: '0.0 = 禁用缺陷系统' },
  ]},
  { key: 'style', icon: '✏️', title: '回复风格', fields: [
    { key: 'style.max_reply_chars', label: '最大字数', type: 'number',
      tip: '单条回复最大字数。越短越像手机打字，越长越能表达复杂意思。建议 20~50', hint: '建议 20~50' },
    { key: 'style.omit_subject', label: '省略主语', type: 'select', options: [{value:true,label:'是'},{value:false,label:'否'}],
      tip: 'true = "我觉得很无聊" → "无聊"，"我在想事情" → "在想事情"' },
    { key: 'style.punctuation_style', label: '标点风格', type: 'select', options: [{value:'casual',label:'日常 (不加句号)'},{value:'formal',label:'正式 (正常标点)'}],
      tip: 'casual = 日常发言不加句号，用换行代替停顿；formal = 使用正常标点符号' },
  ]},
  { key: 'vision', icon: '👁️', title: '识图功能', fields: [
    { key: 'vision.api_key', label: 'API Key', type: 'text',
      tip: '识图 API key，留空则禁用识图功能。图片消息会被忽略', hint: '留空 = 禁用' },
    { key: 'vision.base_url', label: 'Base URL', type: 'text',
      tip: '识图 API 地址，如 https://ark.cn-beijing.volces.com/api/v3' },
    { key: 'vision.model', label: '模型', type: 'text',
      tip: '识图模型名称，如 doubao-seed-1-8-251228' },
    { key: 'vision.max_tokens', label: '最大 Token', type: 'number',
      tip: '识图响应的最大 token 数' },
  ]},
  { key: 'log', icon: '📝', title: '日志', fields: [
    { key: 'log.enabled', label: '启用', type: 'select', options: [{value:true,label:'开启'},{value:false,label:'关闭'}],
      tip: '是否启用日志文件输出。日志写入 data/plugin_ai_chat/logs/ 目录，按天滚动' },
    { key: 'log.level', label: '级别', type: 'select', options: [{value:'debug',label:'debug (全部)'},{value:'info',label:'info (关键流程)'},{value:'warn',label:'warn (警告)'},{value:'error',label:'error (仅错误)'}],
      tip: 'debug=所有详细信息；info=关键流程；warn=仅警告和错误；error=仅错误' },
  ]},
]

const cfg = ref(JSON.parse(JSON.stringify(DEFAULTS)))
const loading = ref(true)
const saving = ref(false)
const saveMsg = ref('')
const saveOk = ref(true)
const expanded = ref({})

function toggle(key) { expanded.value[key] = !expanded.value[key] }

function getFieldVal(f) {
  const parts = f.key.split('.')
  let v = cfg.value
  for (const p of parts) { v = v?.[p] }
  return v
}

function setFieldVal(f, val) {
  const parts = f.key.split('.')
  let obj = cfg.value
  for (let i = 0; i < parts.length - 1; i++) {
    if (!obj[parts[i]]) obj[parts[i]] = {}
    obj = obj[parts[i]]
  }
  obj[parts[parts.length - 1]] = val
}

function arrToText(arr) { return (arr || []).join('\n') }
function textToArr(text) { return text.split('\n').map(s => s.trim()).filter(Boolean).map(Number).filter(n => !isNaN(n)) }

const whitelistText = ref('')
const blacklistText = ref('')
const autoStartText = ref('')

async function load() {
  loading.value = true
  try {
    const data = await api('/api/config')
    const merged = JSON.parse(JSON.stringify(DEFAULTS))
    for (const k of Object.keys(data)) {
      if (typeof data[k] === 'object' && !Array.isArray(data[k]) && merged[k] && typeof merged[k] === 'object') {
        merged[k] = { ...merged[k], ...data[k] }
      } else {
        merged[k] = data[k]
      }
    }
    cfg.value = merged
    whitelistText.value = arrToText(merged.whitelist)
    blacklistText.value = arrToText(merged.blacklist)
    autoStartText.value = arrToText(merged.auto_start_users)
  } catch (e) {
    saveMsg.value = '加载失败: ' + e.message
    saveOk.value = false
  } finally {
    loading.value = false
  }
}

async function save() {
  saving.value = true
  saveMsg.value = ''
  cfg.value.whitelist = textToArr(whitelistText.value)
  cfg.value.blacklist = textToArr(blacklistText.value)
  cfg.value.auto_start_users = textToArr(autoStartText.value)
  try {
    const res = await api('/api/config', { method: 'PUT', body: JSON.stringify(cfg.value) })
    saveMsg.value = res.message || '已保存 ✓'
    saveOk.value = true
    setTimeout(() => { saveMsg.value = '' }, 4000)
  } catch (e) {
    saveMsg.value = '保存失败: ' + e.message
    saveOk.value = false
  } finally {
    saving.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.top-bar {
  display: flex; justify-content: space-between; align-items: center;
  margin-bottom: 16px; position: sticky; top: 0; z-index: 10;
  background: var(--bg); padding: 8px 0;
}

.top-bar h2 { font-size: 18px; font-weight: 600; margin: 0; }

.top-actions { display: flex; align-items: center; gap: 10px; }

.save-ok { color: var(--success); font-size: 13px; font-weight: 500; }
.save-err { color: var(--danger); font-size: 13px; font-weight: 500; }

.section {
  background: #fff; border-radius: var(--radius); padding: 16px;
  margin-bottom: 10px; box-shadow: var(--shadow);
}

.section.highlight { border-left: 3px solid var(--accent); }

h3 { font-size: 14px; margin: 0 0 12px; font-weight: 600; color: var(--text); }
h3.collapsible { cursor: pointer; user-select: none; margin: 0; padding: 2px 0; transition: color .15s; }
h3.collapsible:hover { color: var(--accent); }

.desc { font-size: 12px; color: var(--text-dim); margin: -6px 0 12px; line-height: 1.5; }
.section-body { padding-top: 4px; }

.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 14px;
}

.field-grid.cols-3 {
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
}

.field {
  display: flex; flex-direction: column; gap: 4px;
  /* tooltip via title on .field */
}

.field label {
  font-size: 12px; font-weight: 500; color: var(--text-dim);
  cursor: help; /* 手形提示可悬停 */
}

.range-val {
  font-family: 'SFMono-Regular', Consolas, monospace;
  color: var(--accent); font-weight: 600; font-size: 12px;
}

.field input[type="text"],
.field input[type="password"],
.field input[type="number"],
.field select,
.field textarea {
  background: var(--accent-light); border: 1.5px solid var(--border); color: var(--text);
  padding: 8px 10px; border-radius: var(--radius); font-size: 13px;
  outline: none; transition: border .15s;
}

.field input:focus, .field select:focus, .field textarea:focus { border-color: var(--accent); }
.field input[type="range"] { accent-color: var(--accent); }
.field textarea { font-family: 'SFMono-Regular', Consolas, monospace; resize: vertical; font-size: 12px; }
.hint { font-size: 11px; color: var(--text-dim); }

.btn {
  border: none; padding: 10px 20px; border-radius: var(--radius);
  cursor: pointer; font-size: 13px; font-weight: 500; transition: all .15s;
}
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background: var(--accent); color: #fff; }
.btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
.btn-outline { background: transparent; border: 1.5px solid var(--border); color: var(--text); }
</style>
