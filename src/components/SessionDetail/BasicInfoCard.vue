<template>
  <n-card title="基本信息" class="info-card">
    <template #header-extra>
      <n-icon size="20" color="#07c160">
        <svg viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 12C14.21 12 16 10.21 16 8C16 5.79 14.21 4 12 4C9.79 4 8 5.79 8 8C8 10.21 9.79 12 12 12ZM12 14C9.33 14 4 15.34 4 18V20H20V18C20 15.34 14.67 14 12 14Z"/>
        </svg>
      </n-icon>
    </template>
    <div class="info-grid">
      <div class="info-item">
        <span class="info-label">微信ID</span>
        <span class="info-value">{{ session.wx_id }}</span>
      </div>
      <div class="info-item">
        <span class="info-label">昵称</span>
        <span class="info-value">{{ session.wx_name }}</span>
      </div>
      <div class="info-item">
        <span class="info-label">邮箱</span>
        <span class="info-value">{{ session.wx_email }}</span>
      </div>
      <div class="info-item">
        <span class="info-label">手机号</span>
        <span class="info-value">{{ session.wx_mobile }}</span>
      </div>
    </div>
  </n-card>
</template>

<script setup lang="ts">
import { NCard, NIcon } from 'naive-ui';
import type { SessionData } from '../Session/SessionSidebar.vue';

defineProps<{
  session: SessionData;
}>();
</script>

<style scoped>
.info-card {
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
  overflow: hidden;
  transition: all 0.2s ease;
  background: white;
}

.info-card:hover {
  transform: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.info-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 16px;
}

.info-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
  background: #f8f8f8;
  border-radius: 6px;
  border-left: 3px solid #07c160;
}

.info-label {
  font-size: 12px;
  font-weight: 500;
  color: #999999;
  text-transform: none;
  letter-spacing: 0;
}

.info-value {
  font-size: 14px;
  font-weight: 400;
  color: #333333;
  word-break: break-all;
}

/* 卡片标题样式 */
.n-card .n-card-header {
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
  font-weight: 500;
  color: #333333;
}

.n-card .n-card-header .n-card-header__extra {
  display: flex;
  align-items: center;
}

.n-card {
  border-radius: 8px;
}
</style>
