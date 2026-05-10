<template>
  <div>
    <h2>📊 配额追踪</h2>

    <!-- 用户兴趣 -->
    <h3>🎯 用户兴趣</h3>
    <div v-if="!Object.keys(interest).length" class="empty">暂无兴趣数据</div>
    <table v-else>
      <thead><tr><th>用户</th><th>兴趣分</th><th>标记次数</th><th>最后回顾</th><th>最后消息</th></tr></thead>
      <tbody>
        <tr v-for="(info, uid) in interest" :key="uid">
          <td class="mono">{{ uid }}</td>
          <td>
            <div class="bar-wrap">
              <div class="bar-fill" :style="{ width: (info.score * 100) + '%' }"></div>
              <span class="bar-label">{{ (info.score * 100).toFixed(0) }}%</span>
            </div>
          </td>
          <td>{{ info.marked_count }}</td>
          <td>{{ fmtTime(info.last_reviewed) }}</td>
          <td>{{ fmtTime(info.last_message) }}</td>
        </tr>
      </tbody>
    </table>

    <!-- 段历史 -->
    <h3>📋 段历史</h3>
    <div class="toolbar">
      <select v-model="selectedGroup" @change="loadSegments">
        <option value="">选择群聊</option>
        <option v-for="g in groups" :key="g" :value="g">群 {{ g }}</option>
      </select>
    </div>
    <div v-if="!selectedGroup" class="empty">请选择群聊查看段历史</div>
    <div v-else-if="!segments.length" class="empty">暂无段记录</div>
    <table v-else>
      <thead><tr><th>时间</th><th>总消息</th><th>已回复</th><th>跳过</th><th>已回顾</th><th></th></tr></thead>
      <tbody>
        <template v-for="seg in segments" :key="seg.segment_start">
          <tr @click="toggleExpand(seg.segment_start)" class="clickable">
            <td>{{ fmtTime(seg.segment_start) }}</td>
            <td>{{ seg.messages.length }}</td>
            <td class="replied">{{ seg.messages.filter(m => m.replied).length }}</td>
            <td class="skipped">{{ seg.messages.filter(m => !m.replied).length }}</td>
            <td>{{ seg.reviewed ? '✅' : '⏳' }}</td>
            <td>{{ expanded === seg.segment_start ? '▼' : '▶' }}</td>
          </tr>
          <tr v-if="expanded === seg.segment_start" class="detail-row">
            <td colspan="6">
              <table class="inner-table">
                <thead><tr><th>用户</th><th>消息</th><th>状态</th><th>原因</th></tr></thead>
                <tbody>
                  <tr v-for="(m, i) in seg.messages" :key="i">
                    <td class="mono">{{ m.user_id }}</td>
                    <td class="msg-text">{{ truncate(m.message, 60) }}</td>
                    <td><span :class="m.replied ? 'tag-replied' : 'tag-skipped'">{{ m.replied ? '已回复' : '跳过' }}</span></td>
                    <td class="reason">{{ m.reason || '-' }}</td>
                  </tr>
                </tbody>
              </table>
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>
<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const groups = ref([])
const selectedGroup = ref('')
const segments = ref([])
const interest = ref({})
const expanded = ref(null)

function fmtTime(ts) {
  if (!ts) return '-'
  return new Date(ts * 1000).toLocaleString('zh-CN')
}
function truncate(s, n) {
  if (!s) return ''
  return s.length > n ? s.slice(0, n) + '...' : s
}
function toggleExpand(segStart) {
  expanded.value = expanded.value === segStart ? null : segStart
}

async function loadGroups() {
  const data = await api('/api/quota/segments')
  groups.value = data.groups || []
}
async function loadSegments() {
  if (!selectedGroup.value) { segments.value = []; return }
  const data = await api('/api/quota/segments/' + selectedGroup.value)
  segments.value = data.segments || []
}
async function loadInterest() {
  const data = await api('/api/quota/interest')
  interest.value = data.users || {}
}

onMounted(() => { loadGroups(); loadInterest() })
</script>
<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
h3 { font-size: 14px; margin: 24px 0 8px; color: var(--text-dim); }
.toolbar { display: flex; gap: 8px; margin-bottom: 16px; }
.toolbar select { background: var(--surface); border: 1.5px solid var(--border); color: var(--text); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; outline: none; }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); margin-bottom: 16px; }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; text-transform: uppercase; }
tr:hover { background: var(--surface2); }
.clickable { cursor: pointer; }
.mono { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 12px; }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
.replied { color: var(--success); font-weight: 600; }
.skipped { color: var(--text-dim); }
.detail-row td { padding: 0; background: var(--surface); }
.inner-table { box-shadow: none; margin: 0; }
.inner-table th { background: var(--surface2); font-size: 11px; }
.msg-text { max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.reason { color: var(--text-dim); font-size: 12px; max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag-replied { background: var(--success); color: #fff; padding: 2px 8px; border-radius: 10px; font-size: 11px; }
.tag-skipped { background: var(--border); color: var(--text-dim); padding: 2px 8px; border-radius: 10px; font-size: 11px; }
.bar-wrap { position: relative; width: 100px; height: 20px; background: var(--surface); border-radius: 10px; overflow: hidden; }
.bar-fill { height: 100%; background: linear-gradient(90deg, #ec4899, #8b5cf6); border-radius: 10px; transition: width 0.3s; }
.bar-label { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 600; color: var(--text); }
</style>
