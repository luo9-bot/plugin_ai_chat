<template>
  <div>
    <div class="tab-bar">
      <button class="tab-btn" :class="{ active: tab === 'diary' }" @click="tab = 'diary'">
        日记 <span class="tab-count">{{ diary.length }}</span>
      </button>
      <button class="tab-btn" :class="{ active: tab === 'persons' }" @click="tab = 'persons'">
        人物档案 <span class="tab-count">{{ persons.length }}</span>
      </button>
    </div>

    <template v-if="tab === 'diary'">
      <div v-if="diary.length" class="diary-list">
        <div v-for="e in diary" :key="e.id" class="card diary-card">
          <div class="diary-head">
            <span class="diary-date">{{ e.date }}</span>
            <span class="diary-about" v-if="e.about">关于 {{ e.about }}</span>
          </div>
          <div class="diary-content">{{ e.content }}</div>
          <div class="diary-feeling" v-if="e.feeling">{{ e.feeling }}</div>
        </div>
      </div>
      <div v-else class="card"><div class="empty">还没有日记</div></div>
    </template>

    <template v-else>
      <div v-if="persons.length" class="person-grid">
        <div v-for="p in persons" :key="p.user_id" class="card person-card">
          <div class="person-head">
            <span class="person-name">{{ p.file.display_name || `用户 ${p.user_id}` }}</span>
            <span class="person-uid">{{ p.user_id }}</span>
          </div>
          <div class="person-address" v-if="p.file.address">她叫他：{{ p.file.address }}</div>
          <dl class="person-fields">
            <div class="field" v-if="p.file.impression">
              <dt>印象</dt><dd>{{ p.file.impression }}</dd>
            </div>
            <div class="field" v-if="p.file.my_feeling">
              <dt>她的感觉</dt><dd>{{ p.file.my_feeling }}</dd>
            </div>
            <div class="field" v-if="p.file.mode">
              <dt>相处</dt><dd>{{ p.file.mode }}</dd>
            </div>
          </dl>
          <div class="person-section" v-if="p.file.want_to_say?.length">
            <div class="section-label">想对他说</div>
            <ul class="say-list">
              <li v-for="(s, i) in p.file.want_to_say" :key="i">{{ s }}</li>
            </ul>
          </div>
          <div class="person-section" v-if="p.file.memories?.length">
            <div class="section-label">共同经历</div>
            <ul class="memory-list">
              <li v-for="(m, i) in p.file.memories" :key="i">{{ m }}</li>
            </ul>
          </div>
        </div>
      </div>
      <div v-else class="card"><div class="empty">她还没有记住任何人</div></div>
    </template>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const tab = ref('diary')
const diary = ref([])
const persons = ref([])

async function load() {
  try { diary.value = (await api('/api/mind/diary')).entries || [] } catch {}
  try { persons.value = (await api('/api/mind/persons')).persons || [] } catch {}
}
onMounted(() => { load(); window.addEventListener('refresh-all', load) })
</script>

<style scoped>
.tab-bar {
  display: flex; gap: 8px; margin-bottom: 18px;
}
.tab-btn {
  display: inline-flex; align-items: center; gap: 6px;
  padding: 7px 16px; border-radius: var(--radius-full);
  border: 1px solid var(--border); background: var(--bg-alt);
  color: var(--text-2); font-size: 13px; font-weight: 500; cursor: pointer;
  transition: var(--transition-fast);
}
.tab-btn:hover { border-color: var(--primary); color: var(--primary); }
.tab-btn.active { background: var(--primary); border-color: var(--primary); color: var(--surface-solid); }
.tab-count {
  min-width: 18px; height: 18px; padding: 0 6px;
  display: inline-flex; align-items: center; justify-content: center;
  border-radius: var(--radius-full); font-size: 11px;
  background: var(--border-light); color: var(--text-2);
  font-variant-numeric: tabular-nums;
}
.tab-btn.active .tab-count { background: rgba(255, 255, 255, 0.25); color: var(--surface-solid); }

/* 日记 */
.diary-list { display: flex; flex-direction: column; gap: 12px; }
.diary-card { margin-bottom: 0; }
.diary-head { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
.diary-date {
  font-size: 12px; font-weight: 600; color: var(--primary);
  padding: 2px 10px; border-radius: var(--radius-full);
  background: var(--primary-subtle); font-variant-numeric: tabular-nums;
}
.diary-about { font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }
.diary-content { font-size: 13px; line-height: 1.7; white-space: pre-wrap; word-break: break-word; }
.diary-feeling {
  margin-top: 10px; padding: 8px 12px; border-radius: var(--radius-xs);
  background: var(--primary-subtle); color: var(--primary);
  font-size: 12px; line-height: 1.5;
}

/* 人物档案 */
.person-grid {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 14px;
}
.person-card { margin-bottom: 0; }
.person-head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
.person-name { font-size: 15px; font-weight: 700; color: var(--text); }
.person-uid { font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }
.person-address { margin-top: 4px; font-size: 12px; color: var(--primary); font-weight: 500; }
.person-fields { margin: 12px 0 0; display: flex; flex-direction: column; gap: 8px; }
.field { display: flex; gap: 10px; align-items: baseline; }
.field dt { flex-shrink: 0; width: 58px; font-size: 11px; color: var(--text-3); }
.field dd { margin: 0; font-size: 13px; line-height: 1.6; color: var(--text); }
.person-section { margin-top: 12px; }
.section-label {
  font-size: 11px; font-weight: 600; color: var(--text-3);
  text-transform: uppercase; letter-spacing: 0.6px; margin-bottom: 6px;
}
.say-list, .memory-list { margin: 0; padding: 0 0 0 16px; display: flex; flex-direction: column; gap: 4px; }
.say-list li, .memory-list li { font-size: 12px; line-height: 1.6; color: var(--text-2); }
.say-list li { color: var(--accent); }

.empty { text-align: center; padding: 40px; color: var(--text-3); font-size: 13px; }
</style>
