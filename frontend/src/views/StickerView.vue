<template>
  <div>
    <h2>😊 表情包管理</h2>
    <div class="toolbar">
      <span class="count">共 {{ data.total ?? 0 }} 个，已注册 {{ data.registered ?? 0 }}</span>
    </div>
    <div v-if="!stickers.length" class="empty">📭 暂无表情包</div>
    <table v-else>
      <thead><tr><th>描述</th><th>情绪标签</th><th>类型</th><th>使用次数</th><th>状态</th><th>操作</th></tr></thead>
      <tbody>
        <tr v-for="s in stickers" :key="s.hash">
          <td class="truncate">{{ s.description || '-' }}</td>
          <td><span v-for="e in s.emotions" :key="e" class="tag" style="margin-right:4px">{{ e }}</span></td>
          <td><span :class="'badge badge-' + (s.is_builtin ? 'permanent' : 'normal')">{{ s.is_builtin ? '内置' : '收集' }}</span></td>
          <td>{{ s.query_count || 0 }}</td>
          <td>
            <span v-if="!s.is_registered" class="tag" style="background:#f3e8ff;color:#a855f7">未注册</span>
            <span v-else-if="s.is_banned" class="tag" style="background:#fee2e2;color:#ef4444">已封禁</span>
            <span v-else class="tag" style="background:#d1fae5;color:#10b981">可用</span>
          </td>
          <td class="actions">
            <button class="btn btn-sm" :class="s.is_banned ? 'btn-success' : 'btn-warning'" @click="toggleBan(s.hash)">
              {{ s.is_banned ? '解封' : '封禁' }}
            </button>
            <button class="btn btn-danger btn-sm" @click="del(s.hash)">删除</button>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'
const data = ref({})
const stickers = computed(() => data.value.stickers || [])
async function load() { data.value = await api('/api/sticker') }
async function toggleBan(hash) { await api('/api/sticker/' + hash, { method: 'POST' }); load() }
async function del(hash) { if (!confirm('确定删除此表情包？')) return; await api('/api/sticker/' + hash, { method: 'DELETE' }); load() }
onMounted(load)
</script>

<style scoped>
h2 { font-size: 18px; margin-bottom: 16px; font-weight: 600; }
.toolbar { display: flex; gap: 12px; margin-bottom: 16px; align-items: center; }
.count { font-size: 13px; color: var(--text-dim); }
table { width: 100%; border-collapse: collapse; font-size: 13px; background: var(--surface); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow); }
th, td { text-align: left; padding: 10px 14px; border-bottom: 1px solid var(--accent-light); }
th { background: var(--accent-light); color: var(--accent); font-weight: 600; font-size: 12px; }
tr:hover { background: var(--surface2); }
.truncate { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { display: inline-block; padding: 2px 8px; border-radius: 12px; font-size: 11px; background: var(--accent-light); color: var(--accent); }
.badge { display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; font-weight: 500; }
.badge-permanent { background: #fee2e2; color: #ef4444; }
.badge-normal { background: #f3e8ff; color: #a855f7; }
.actions { display: flex; gap: 6px; }
.btn { border: none; padding: 6px 14px; border-radius: var(--radius); cursor: pointer; font-size: 12px; font-weight: 500; transition: all .15s; }
.btn-sm { padding: 5px 12px; font-size: 11px; }
.btn-warning { background: var(--warning-light); color: var(--warning); }
.btn-success { background: var(--success-light); color: var(--success); }
.btn-danger { background: var(--danger-light); color: var(--danger); }
.empty { text-align: center; padding: 40px; color: var(--text-dim); }
</style>
