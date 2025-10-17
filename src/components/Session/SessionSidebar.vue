<template>
  <n-layout-sider width="320" bordered class="session-sidebar">
    <div class="session-list">
      <div
        v-for="s in sessions"
        :key="s.id"
        :class="['session-item', selectedId === s.id ? 'session-item-active' : '']"
        @click="$emit('selectSession', s)"
      >
        <div class="session-avatar">
           <img
             :src="resolvedAvatars[s.id] || buildAvatarUrl(s)"
             @error="onAvatarError(s)"
           />
        </div>
        
        <div class="session-info">
          <div class="session-name">{{ s.name }}</div>
          <div class="session-desc">{{ s.desc }}</div>
        </div>
      </div>
    </div>

    <!-- Removed bottom logout area to keep sidebar minimal per spec -->
  </n-layout-sider>
</template>

<script setup lang="ts">
import { NLayoutSider, NIcon } from 'naive-ui';
import { ref, onMounted, watch } from 'vue';
import { endpoint } from '@/common/login';
import type { Session } from '@/models/session';

const host = endpoint();

const props = defineProps<{
  sessions: Session[];
  selectedId?: number;
}>();

defineEmits<{
  selectSession: [session: Session];
}>();

// Map of avatar srcs keyed by session id
const resolvedAvatars = ref<Record<number, string>>({});

const buildAvatarUrl = (s: Session) => `${host}/api/resources/relative-resource?relative_path=${s.wx_id}/head_image/${s.wx_id}.jpg&session_id=${s.id}`;
const getDefaultAvatar = (name?: string) => `https://ui-avatars.com/api/?name=${encodeURIComponent(name || 'U')}&background=random&size=128`;

const onAvatarError = (s: Session) => {
  resolvedAvatars.value[s.id] = getDefaultAvatar(s.name);
};

onMounted(() => {
  props.sessions.forEach((s) => {
    resolvedAvatars.value[s.id] = buildAvatarUrl(s);
  });
});

watch(
  () => props.sessions.map(s => ({ id: s.id, wx_id: s.wx_id })),
  (list) => {
    for (const { id } of list) {
      const s = props.sessions.find(ss => ss.id === id);
      if (s) resolvedAvatars.value[id] = buildAvatarUrl(s);
    }
  },
  { deep: true }
)
</script>

<style scoped>
/* 左侧会话列表样式 - 微信风格浅色 */
.session-sidebar {
  background: #ffffff;
  box-shadow: 2px 0 8px rgba(0, 0, 0, 0.1);
  border: none !important;
  display: flex;
  flex-direction: column;
  height: 100%;
}

.sidebar-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 20px 16px;
  background: #ffffff;
  color: #333333;
  border-bottom: 1px solid #e7e7e7;
}

.flex-grow {
  flex: 1;
}

/* removed sidebar add button styles */

.sidebar-title {
  font-size: 16px;
  font-weight: 500;
  color: #333333;
}

.session-list {
  padding: 0;
  overflow-y: auto;
  background: #ffffff;
  flex: 1 1 auto;
}

.session-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  margin: 0;
  border-radius: 0;
  cursor: pointer;
  transition: background-color 0.2s ease;
  background: #ffffff;
  border: none;
  border-bottom: 1px solid #f0f0f0;
  box-shadow: none;
  position: relative;
}

.session-item:hover {
  transform: none;
  background: #f5f5f5;
  box-shadow: none;
}

.session-item-active {
  background: #07c160;
  border-color: #07c160;
  color: white;
  box-shadow: none;
}

.session-item-active:hover {
  background: #06ad56;
}

.session-avatar {
  position: relative;
  flex-shrink: 0;
  img {
    width: 48px;
    height: 48px;
    border-radius: 6px;
    object-fit: cover;
  }
}

.session-status {
  position: absolute;
  bottom: 0;
  right: 0;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  border: 2px solid #ffffff;
  box-shadow: none;
}

.session-status.online {
  background: #07c160;
}

.session-status.offline {
  background: #999999;
}

.session-info {
  flex: 1;
  min-width: 0;
}

.session-name {
  font-size: 16px;
  font-weight: 500;
  margin-bottom: 4px;
  color: #333333;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-item-active .session-name {
  color: white;
}

.session-desc {
  font-size: 13px;
  color: #999999;
  margin-bottom: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-item-active .session-desc {
  color: rgba(255, 255, 255, 0.9);
}

.session-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.session-time {
  font-size: 11px;
  color: #999999;
  flex-shrink: 0;
}

.session-item-active .session-time {
  color: rgba(255, 255, 255, 0.8);
}

/* 滚动条样式 - 微信风格浅色 */
.session-list::-webkit-scrollbar {
  width: 4px;
}

.session-list::-webkit-scrollbar-track {
  background: #ffffff;
}

.session-list::-webkit-scrollbar-thumb {
  background: #cccccc;
  border-radius: 2px;
}

.session-list::-webkit-scrollbar-thumb:hover {
  background: #aaaaaa;
}

/* Tag样式调整 */
.session-item-active .n-tag {
  background: rgba(255, 255, 255, 0.2) !important;
  color: white !important;
  border: 1px solid rgba(255, 255, 255, 0.3) !important;
}

/* 头像圆角样式 */
.n-avatar {
  border-radius: 6px !important;
}

.n-tag {
  border-radius: 12px;
}
</style>
