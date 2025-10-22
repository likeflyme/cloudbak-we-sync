<template>
  <div class="action-section">
    <n-space size="large">
      <!-- 同步按钮 -->
      <n-button
        :loading="syncing"
        :disabled="syncing"
        type="primary"
        size="large"
        class="action-btn wechat-btn"
        @click="startSync"
      >
        <template #icon>
          <n-icon v-if="!syncing">
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M12,18A6,6 0 0,1 6,12C6,11 6.25,10.03 6.7,9.2L5.24,7.74C4.46,8.97 4,10.43 4,12A8,8 0 0,0 12,20V23L16,19L12,15M12,4V1L8,5L12,9V6A6,6 0 0,1 18,12C18,13 17.75,13.97 17.3,14.8L18.76,16.26C19.54,15.03 20,13.57 20,12A8,8 0 0,0 12,4Z" />
            </svg>
          </n-icon>
          <n-icon v-else class="spin">
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 4a8 8 0 0 1 8 8h2A10 10 0 0 0 12 2v2Zm0 16a8 8 0 0 1-8-8H2a10 10 0 0 0 10 10v-2Z" />
            </svg>
          </n-icon>
        </template>
        <span v-if="!syncing">开始同步</span>
        <span v-else>正在同步...</span>
      </n-button>

      <!-- 删除按钮 -->
      <n-popconfirm
        @positive-click="handleDelete"
        negative-text="取消"
        positive-text="确认删除"
        placement="top"
      >
        <template #trigger>
          <n-button
            type="error"
            size="large"
            class="action-btn"
            secondary
            :disabled="syncing"
          >
            <template #icon>
              <n-icon>
                <svg viewBox="0 0 24 24" fill="currentColor">
                  <path d="M19,4H15.5L14.5,3H9.5L8.5,4H5V6H19M6,19A2,2 0 0,0 8,21H16A2,2 0 0,0 18,19V7H6V19Z" />
                </svg>
              </n-icon>
            </template>
            删除会话
          </n-button>
        </template>
        <div class="delete-confirm">
          <p>确认要删除该会话吗？</p>
          <p class="delete-warning">此操作不可撤销，请谨慎操作</p>
        </div>
      </n-popconfirm>
    </n-space>
  </div>
</template>

<script setup lang="ts">
import { NButton, NIcon, NPopconfirm, NSpace } from 'naive-ui'

interface Emits {
  (e: 'sync', key: string): void
  (e: 'delete'): void
}

const emit = defineEmits<Emits>()
const props = defineProps<{ syncing: boolean }>()

const startSync = () => {
  if (!props.syncing) emit('sync', 'sync')
}

const handleDelete = () => {
  emit('delete')
}
</script>

<style scoped>
.action-section {
  display: flex;
  justify-content: center;
  padding: 20px;
  background: white;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
}

.action-btn {
  border-radius: 6px;
  padding: 10px 24px;
  font-weight: 500;
  font-size: 14px;
  box-shadow: none;
  transition: all 0.2s ease;
  border: none;
}

.wechat-btn {
  background: #07c160 !important;
  color: white !important;
}

.wechat-btn:hover {
  background: #06ad56 !important;
  box-shadow: 0 2px 4px rgba(7, 193, 96, 0.3);
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.delete-confirm p {
  margin: 0 0 8px 0;
  color: #333;
  font-size: 14px;
}

.delete-warning {
  font-size: 12px;
  color: #ff3b30;
}

/* 响应式设计 */
@media (max-width: 768px) {
  .action-section {
    padding: 16px;
  }

  .action-btn {
    padding: 8px 20px;
    font-size: 13px;
  }
}
</style>
