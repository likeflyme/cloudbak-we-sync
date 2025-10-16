<template>
  <div class="content-wrapper">
    <!-- 用户信息头部 -->
    <UserHeader 
      :session="session" 
      @toggle-auto-sync="$emit('toggle-auto-sync')"
    />

    <!-- 详细信息卡片 -->
    <div class="info-cards">
      <BasicInfoCard :session="session" />
      <StorageInfoCard :session="session" />
      <KeyInfoCard 
        :session="session" 
        :key-visibility="keyVisibility"
        @toggle-key-visibility="$emit('toggle-key-visibility', $event)"
        @copy-key="$emit('copy-key', $event)"
      />
    </div>

    <!-- 同步过滤配置 -->
    <FilterConfigCard 
      :sync-filters="session.syncFilters"
      @update:syncFilters="(v: string) => $emit('update:syncFilters', v)"
    />

    <!-- 操作按钮区域 -->
    <ActionButtons 
      @sync="(key: string) => $emit('sync', key)"
      @delete="$emit('delete')"
    />
  </div>
</template>

<script setup lang="ts">
import UserHeader from '@/components/SessionDetail/UserHeader.vue'
import BasicInfoCard from '@/components/SessionDetail/BasicInfoCard.vue'
import StorageInfoCard from '@/components/SessionDetail/StorageInfoCard.vue'
import KeyInfoCard from '@/components/SessionDetail/KeyInfoCard.vue'
import FilterConfigCard from '@/components/SessionDetail/FilterConfigCard.vue'
import ActionButtons from '@/components/SessionDetail/ActionButtons.vue'
import type { Session } from '@/models/session'

interface KeyVisibility {
  data_key: boolean
  aes_key: boolean
  xor_key: boolean
}

defineProps<{
  session: Session
  keyVisibility: KeyVisibility
}>()

defineEmits<{
  (e: 'toggle-auto-sync'): void
  (e: 'toggle-key-visibility', key: 'data_key' | 'aes_key' | 'xor_key'): void
  (e: 'copy-key', key: string): void
  (e: 'update:syncFilters', value: string): void
  (e: 'sync', key: string): void
  (e: 'delete'): void
}>()
</script>

<style scoped>
.content-wrapper {
  padding: 20px;
  max-width: 1000px;
  margin: 0 auto;
}

/* 信息卡片样式 */
.info-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
  gap: 16px;
  margin-bottom: 20px;
}

/* 响应式设计 */
@media (max-width: 1200px) {
  .content-wrapper {
    padding: 16px;
  }
  
  .info-cards {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 768px) {
  .content-wrapper {
    padding: 12px;
  }
}

/* 动画效果 */
@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.content-wrapper > * {
  animation: fadeIn 0.3s ease-out;
}
</style>
