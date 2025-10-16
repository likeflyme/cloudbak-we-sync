<template>
  <n-card title="存储位置" class="info-card">
    <template #header-extra>
      <n-icon size="20" color="#07c160">
        <svg viewBox="0 0 24 24" fill="currentColor">
          <path d="M9.5 3A6.5 6.5 0 0 1 16 9.5C16 11.11 15.41 12.59 14.44 13.73L14.71 14H15.5L20.5 19L19 20.5L14 15.5V14.71L13.73 14.44C12.59 15.41 11.11 16 9.5 16A6.5 6.5 0 0 1 3 9.5A6.5 6.5 0 0 1 9.5 3M9.5 5C7 5 5 7 5 9.5S7 14 9.5 14 14 12 14 9.5 12 5 9.5 5Z"/>
        </svg>
      </n-icon>
    </template>
    <div class="storage-info">
      <div class="storage-path">
        <n-icon size="16" color="#666">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M10,4H4C2.89,4 2,4.89 2,6V18A2,2 0 0,0 4,20H20A2,2 0 0,0 22,18V8C22,6.89 21.1,6 20,6H12L10,4Z"/>
          </svg>
        </n-icon>
        <span>{{ session.wx_dir }}</span>
      </div>
    </div>
  </n-card>
</template>

<script setup lang="ts">
import { NCard, NIcon } from 'naive-ui';
import type { Session } from '@/models/session';

defineProps<{
  session: Session;
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

.storage-info {
  padding: 4px 0;
}

.storage-path {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background: #f8f8f8;
  border-radius: 6px;
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Fira Code', 'Fira Mono', 'Roboto Mono', monospace;
  font-size: 13px;
  color: #333333;
  word-break: break-all;
  border: 1px solid #eeeeee;
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
