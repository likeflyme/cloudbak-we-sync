<template>
  <n-card title="微信数据目录" class="info-card">
    <template #header-extra>
      <n-button size="small" type="primary" secondary @click="openDir">打开目录</n-button>
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
import { NCard, NIcon, NButton } from 'naive-ui';
import type { Session } from '@/models/session';
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  session: Session;
}>();

const stripLongPathPrefix = (p: string) => p.replace(/^\\\\\?\\/, '');

const openDir = async () => {
  if (!props.session?.wx_dir) return;
  try {
    const p = stripLongPathPrefix(props.session.wx_dir);
    await invoke('open_in_os', { path: p, reveal: false })
  } catch (e) {
    console.error('open dir failed', e);
  }
}
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

.storage-actions {
  margin-top: 10px;
  display: flex;
  justify-content: flex-end;
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
