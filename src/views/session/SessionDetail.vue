<template>
  <div class="content-wrapper">
    <!-- 用户信息头部 -->
    <UserHeader 
      :session="session" 
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

    <!-- 同步过滤配置（默认折叠） -->
    <n-collapse v-model:expanded-names="expandedSections">
      <n-collapse-item name="filters" title="高级设置">
        <!-- 自动同步配置卡片（独立 n-card） -->
        <AutoSyncCard :value="session.autoSync" @update:value="$emit('toggle-auto-sync')" />

        <FilterConfigCard 
          :sync-filters="session.syncFilters"
          :session-id="session.id"
          @update:syncFilters="onFilterChange"
        />
        <!-- moved save button into FilterConfigCard -->
      </n-collapse-item>
    </n-collapse>

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
import AutoSyncCard from '@/components/SessionDetail/AutoSyncCard.vue'
import ActionButtons from '@/components/SessionDetail/ActionButtons.vue'
import type { Session } from '@/models/session'
import { NCollapse, NCollapseItem } from 'naive-ui'
import { ref } from 'vue'

interface KeyVisibility {
  data_key: boolean
  aes_key: boolean
  xor_key: boolean
}

defineProps<{
  session: Session
  keyVisibility: KeyVisibility
}>()

const emit = defineEmits<{
  (e: 'toggle-auto-sync'): void
  (e: 'toggle-key-visibility', key: 'data_key' | 'aes_key' | 'xor_key'): void
  (e: 'copy-key', key: string): void
  (e: 'update:syncFilters', value: string): void
  (e: 'sync', key: string): void
  (e: 'delete'): void
}>()

// 默认折叠（空数组）
const expandedSections = ref<string[]>([])

const onFilterChange = (v: string) => {
  emit('update:syncFilters', v)
}
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
