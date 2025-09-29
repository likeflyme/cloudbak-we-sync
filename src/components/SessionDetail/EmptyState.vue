<template>
  <div class="empty-state">
    <n-empty
      :description="description"
      size="huge"
    >
      <template #icon>
        <n-icon size="64" color="#d9d9d9">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M20 6h-2.18c.11-.31.18-.65.18-1a2.996 2.996 0 0 0-5.5-1.65l-.5.67-.5-.68C10.96 2.54 10.05 2 9 2 7.34 2 6 3.34 6 5c0 .35.07.69.18 1H4c-1.11 0-1.99.89-1.99 2L2 19c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2zm-5-2c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zM9 4c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1z"/>
          </svg>
        </n-icon>
      </template>
    </n-empty>
  </div>
</template>

<script setup lang="ts">
import { NEmpty, NIcon } from 'naive-ui'

interface Props {
  description?: string
}

withDefaults(defineProps<Props>(), {
  description: '请从左侧选择一个会话查看详情'
})
</script>

<style scoped>
.empty-state {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: white;
  margin: 20px;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
}

/* 响应式设计 */
@media (max-width: 768px) {
  .empty-state {
    margin: 12px;
  }
}
</style>
