<template>
  <div class="content-wrapper">
    <div class="loading-state">
      <n-spin size="large">
        <template #description>
          <div class="loading-text">{{ message }}</div>
        </template>
      </n-spin>
    </div>
  </div>
</template>

<script setup lang="ts">
import { NSpin } from 'naive-ui'

interface Props {
  message?: string
}

withDefaults(defineProps<Props>(), {
  message: '正在扫描微信数据...'
})
</script>

<style scoped>
.content-wrapper {
  padding: 20px;
  max-width: 1000px;
  margin: 0 auto;
}

.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 400px;
  background: white;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
}

.loading-text {
  margin-top: 16px;
  font-size: 16px;
  color: #666666;
}

/* 响应式设计 */
@media (max-width: 768px) {
  .content-wrapper {
    padding: 12px;
  }
  
  .loading-state {
    height: 300px;
  }
}
</style>
