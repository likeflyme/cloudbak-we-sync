<template>
  <div class="user-header">
    <div class="user-avatar-section">
      <n-avatar
        :size="80"
        :src="session.avatar"
        :fallback-src="getDefaultAvatar(session.wx_acct_name)"
        round
        class="user-main-avatar"
      >
        {{ session.wx_acct_name ? session.wx_acct_name.charAt(0) : 'U' }}
      </n-avatar>
      <div class="user-basic-info">
        <h2 class="user-title">{{ session.name }}</h2>
        <p class="user-subtitle">{{ session.desc }}</p>
        <div class="user-badges">
          <n-tag type="success" size="small" round>
            <template #icon>
              <n-icon>
                <svg viewBox="0 0 24 24" fill="currentColor">
                  <path d="M9 16.17L4.83 12L3.41 13.41L9 19L21 7L19.59 5.59L9 16.17Z"/>
                </svg>
              </n-icon>
            </template>
            在线
          </n-tag>
          <n-tag type="info" size="small" round>{{ session.wx_acct_name }}</n-tag>
        </div>
        
        <!-- 自动同步开关 -->
        <div class="auto-sync-section">
          <div class="auto-sync-label">
            <n-icon size="16" color="rgba(255, 255, 255, 0.8)">
              <svg viewBox="0 0 24 24" fill="currentColor">
                <path d="M12,18A6,6 0 0,1 6,12C6,11 6.25,10.03 6.7,9.2L5.24,7.74C4.46,8.97 4,10.43 4,12A8,8 0 0,0 12,20V23L16,19L12,15M12,4V1L8,5L12,9V6A6,6 0 0,1 18,12C18,13 17.75,13.97 17.3,14.8L18.76,16.26C19.54,15.03 20,13.57 20,12A8,8 0 0,0 12,4Z"/>
              </svg>
            </n-icon>
            <span class="auto-sync-text">自动同步</span>
          </div>
          <n-switch
            :value="session.autoSync"
            @update:value="$emit('toggleAutoSync')"
            size="medium"
            class="auto-sync-switch"
          >
            <template #checked>开启</template>
            <template #unchecked>关闭</template>
          </n-switch>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { NAvatar, NTag, NIcon, NSwitch } from 'naive-ui';
import type { Session } from '@/models/session';

defineProps<{
  session: Session;
}>();

defineEmits<{
  toggleAutoSync: [];
}>();

const getDefaultAvatar = (name?: string) => {
  return `https://ui-avatars.com/api/?name=${encodeURIComponent(name || 'U')}&background=random&size=128`
}
</script>

<style scoped>
.user-header {
  background: linear-gradient(135deg, #07c160 0%, #06ad56 100%);
  border-radius: 8px;
  padding: 24px;
  margin-bottom: 20px;
  box-shadow: 0 2px 8px rgba(7, 193, 96, 0.15);
  color: white;
}

.user-avatar-section {
  display: flex;
  align-items: center;
  gap: 20px;
}

.user-main-avatar {
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  border: 3px solid rgba(255, 255, 255, 0.2);
}

.user-basic-info {
  flex: 1;
}

.user-title {
  font-size: 24px;
  font-weight: 500;
  margin: 0 0 6px 0;
  color: white;
}

.user-subtitle {
  font-size: 14px;
  opacity: 0.9;
  margin: 0 0 12px 0;
  color: white;
}

.user-badges {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

/* 自动同步开关样式 */
.auto-sync-section {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 16px;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  backdrop-filter: blur(10px);
}

.auto-sync-label {
  display: flex;
  align-items: center;
  gap: 8px;
}

.auto-sync-text {
  color: rgba(255, 255, 255, 0.9);
  font-size: 14px;
  font-weight: 500;
}

/* 自动同步开关样式调整 */
.auto-sync-switch .n-switch--active {
  background-color: rgba(255, 255, 255, 0.3) !important;
}

.auto-sync-switch .n-switch__rail {
  background-color: rgba(255, 255, 255, 0.2);
}

.auto-sync-switch .n-switch--active .n-switch__rail {
  background-color: rgba(255, 255, 255, 0.3);
}

.auto-sync-switch .n-switch__button {
  background-color: white;
}

.auto-sync-switch .n-switch__checked,
.auto-sync-switch .n-switch__unchecked {
  color: rgba(255, 255, 255, 0.9);
  font-size: 12px;
  font-weight: 500;
}

/* 微信绿色主题色调整 */
.n-tag--success-type {
  background: #07c160 !important;
  border-color: #07c160 !important;
}

/* 头像圆角样式 */
.n-avatar {
  border-radius: 6px !important;
}

.n-tag {
  border-radius: 12px;
}
</style>
