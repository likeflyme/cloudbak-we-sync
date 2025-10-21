<template>
  <n-card title="同步过滤配置" class="info-card filter-config-card">
    <template #header-extra>
      <n-icon size="20" color="#07c160">
        <svg viewBox="0 0 24 24" fill="currentColor">
          <path d="M14,12V19.88C14.04,20.18 13.94,20.5 13.71,20.71C13.32,21.1 12.69,21.1 12.3,20.71L10.29,18.7C10.06,18.47 9.96,18.16 10,17.87V12H9.97L4.21,4.62C3.87,4.19 3.95,3.56 4.38,3.22C4.57,3.08 4.78,3 5,3V3H19V3C19.22,3 19.43,3.08 19.62,3.22C20.05,3.56 20.13,4.19 19.79,4.62L14.03,12H14Z"/>
        </svg>
      </n-icon>
    </template>
    <div class="filter-config-content">
      <p class="filter-config-desc">配置同步时需要排除的文件和目录，每行一个规则：</p>
      <n-input
        :value="syncFilters"
        @update:value="handleFiltersChange"
        type="textarea"
        placeholder="例如：&#10;*.log&#10;*.tmp&#10;node_modules/&#10;.git/&#10;temp/"
        :rows="6"
        class="filter-textarea"
      />
      <div class="filter-config-help">
        <div class="help-item">
          <n-icon size="14" color="#07c160">
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M11,9H13V7H11M12,20C7.59,20 4,16.41 4,12C4,7.59 7.59,4 12,4C16.41,4 20,7.59 20,12C20,16.41 16.41,20 12,20M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2M11,17H13V11H11V17Z"/>
            </svg>
          </n-icon>
          <span>支持通配符：*.log 匹配所有 .log 文件</span>
        </div>
        <div class="help-item">
          <n-icon size="14" color="#07c160">
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M11,9H13V7H11M12,20C7.59,20 4,16.41 4,12C4,7.59 7.59,4 12,4C16.41,4 20,7.59 20,12C20,16.41 16.41,20 12,20M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2M11,17H13V11H11V17Z"/>
            </svg>
          </n-icon>
          <span>目录请以 / 结尾：temp/ 表示目录</span>
        </div>
      </div>
      <div class="filter-actions">
        <n-button type="primary" size="small" @click="saveFilters">保存过滤配置</n-button>
      </div>
    </div>
  </n-card>
</template>

<script setup lang="ts">
import { NCard, NIcon, NInput, NButton, createDiscreteApi } from 'naive-ui'
import { invoke } from '@tauri-apps/api/core'

interface Props {
  syncFilters: string
  sessionId: number
}

interface Emits {
  (e: 'update:syncFilters', value: string): void
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()
const { message } = createDiscreteApi(['message'])

const handleFiltersChange = (value: string) => {
  emit('update:syncFilters', value)
}

const userId = Number(localStorage.getItem('user_id') || '0')

const saveFilters = async () => {
  try {
    await invoke('save_session_filters', { sessionId: props.sessionId, userId, syncFilters: props.syncFilters || '' })
    message.success('已保存过滤配置')
  } catch (e) {
    console.warn('save_session_filters failed', e)
    message.error('保存失败')
  }
}
</script>

<style scoped>
.filter-config-card {
  margin-bottom: 20px;
}

.filter-config-content {
  padding: 8px 0;
}

.filter-config-desc {
  margin: 0 0 16px 0;
  font-size: 14px;
  color: #666666;
  line-height: 1.4;
}

.filter-textarea {
  margin-bottom: 16px;
}

.filter-textarea :deep(.n-input__textarea-el) {
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Fira Code', 'Fira Mono', 'Roboto Mono', monospace !important;
  font-size: 13px !important;
  line-height: 1.4 !important;
  background: #f8f8f8 !important;
  border: 1px solid #e7e7e7 !important;
  border-radius: 4px !important;
  resize: vertical;
}

.filter-textarea :deep(.n-input__textarea-el:focus) {
  border-color: #07c160 !important;
  box-shadow: 0 0 0 2px rgba(7, 193, 96, 0.2) !important;
}

.filter-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 8px;
  margin-bottom: 16px;
}

.filter-config-help {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.help-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: #999999;
  line-height: 1.4;
}

.help-item span {
  color: #666666;
}
</style>
