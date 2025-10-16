<template>
  <n-layout-sider width="320" bordered class="session-sidebar">
    <div class="sidebar-header">
      <n-icon size="24" color="#07c160">
        <svg viewBox="0 0 24 24" fill="currentColor">
          <path d="M12.04 2C6.58 2 2.13 6.45 2.13 11.91C2.13 13.66 2.59 15.36 3.45 16.86L2.05 22L7.3 20.62C8.75 21.41 10.38 21.83 12.04 21.83C17.5 21.83 21.95 17.38 21.95 11.92C21.95 6.45 17.5 2 12.04 2ZM12.05 20.15C10.59 20.15 9.15 19.75 7.89 19L7.55 18.83L4.42 19.65L5.25 16.61L5.06 16.26C4.24 14.96 3.81 13.47 3.81 11.91C3.81 7.37 7.49 3.69 12.04 3.69C16.58 3.69 20.26 7.37 20.26 11.91C20.26 16.45 16.58 20.15 12.05 20.15Z"/>
        </svg>
      </n-icon>
      <span class="sidebar-title">we-sync</span>
      <div class="flex-grow"></div>
      <!-- Removed header logout button -->
      <n-button
        size="small"
        type="primary"
        circle
        @click="$emit('addSession')"
        class="add-btn"
      >
        <template #icon>
          <n-icon>
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M19,13H13V19H11V13H5V11H11V5H13V11H19V13Z"/>
            </svg>
          </n-icon>
        </template>
      </n-button>
    </div>
    
    <div class="session-list">
      <div
        v-for="s in sessions"
        :key="s.id"
        :class="['session-item', selectedId === s.id ? 'session-item-active' : '']"
        @click="$emit('selectSession', s)"
      >
        <div class="session-avatar">
           <img :src="`${host}/api/resources/relative-resource?relative_path=${s.wx_id}/head_image/${s.wx_id}.jpg&session_id=${s.id}`"/>
        </div>
        
        <div class="session-info">
          <div class="session-name">{{ s.name }}</div>
          <div class="session-desc">{{ s.desc }}</div>
          <div class="session-meta">
            <n-tag size="small" type="success" round>{{ s.wx_acct_name || s.wx_name }}</n-tag>
            <span class="session-time">{{ s.lastActive }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Bottom logout area -->
    <div class="sidebar-footer">
      <n-button quaternary type="error" block @click="$emit('logout')">
        <template #icon>
          <n-icon>
            <svg viewBox="0 0 24 24" fill="currentColor">
              <!-- logout/exit icon -->
              <path d="M14.08 15.59L16.67 13H7V11H16.67L14.08 8.41L15.5 7L20.5 12L15.5 17L14.08 15.59Z"/>
              <path d="M3 5H11V7H5V17H11V19H3V5Z"/>
            </svg>
          </n-icon>
        </template>
        退出登录
      </n-button>
    </div>
  </n-layout-sider>
</template>

<script setup lang="ts">
import { NLayoutSider, NIcon, NButton, NTag } from 'naive-ui';
import { ref, onMounted, watch } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { endpoint } from '@/common/login';
import type { Session } from '@/models/session';

const host = endpoint();

const props = defineProps<{
  sessions: Session[];
  selectedId?: number;
}>();

defineEmits<{
  selectSession: [session: Session];
  addSession: [];
  logout: [];
}>();

const stripLongPathPrefix = (p: string) => p.replace(/^\\\\\?\\/, '');
const isLocalPath = (p: string) => /^[a-zA-Z]:[\\/]/.test(p) || p.startsWith('\\\\') || p.startsWith('/');


// Map of resolved avatar strings keyed by session id
const resolvedAvatars = ref<Record<number, string>>({});

const resolveAvatar = async (src: string): Promise<string> => {
  if (!src) return ''
  // handle url-like sources
  if (/^(https?:|data:|asset:|tauri:|file:)/i.test(src)) {
    if (/^file:\/\//i.test(src)) {
      const path = src.replace(/^file:\/\//i, '')
      try { return convertFileSrc(stripLongPathPrefix(path)) } catch { return src }
    }
    return src
  }
  // handle plain local paths
  if (isLocalPath(src)) {
    try {
      return await invoke<string>('load_avatar', { path: stripLongPathPrefix(src) })
    } catch { return src }
  }
  console.log('resolve avatar', src);
  return src
}

onMounted(() => {
  props.sessions.forEach(async (s) => {
    const resolved = await resolveAvatar(s.avatar)
    if (resolved) {
      resolvedAvatars.value[s.id] = resolved
    }
  })
});

// If sessions list changes, resolve avatars (and update when avatar path changes)
watch(
  () => props.sessions.map(s => ({ id: s.id, avatar: s.avatar })),
  async (list) => {
    for (const { id, avatar } of list) {
      const resolved = await resolveAvatar(avatar)
      if (resolved) resolvedAvatars.value[id] = resolved
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

.add-btn {
  background: #07c160 !important;
  border-color: #07c160 !important;
}

.add-btn:hover {
  background: #06ad56 !important;
  border-color: #06ad56 !important;
}

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

/* Bottom footer */
.sidebar-footer {
  padding: 12px 12px 16px;
  border-top: 1px solid #e7e7e7;
  background: #ffffff;
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
