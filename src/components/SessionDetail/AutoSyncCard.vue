<template>
  <n-card title="自动同步" class="info-card auto-sync-card">
    <div class="auto-sync-row">
      <div class="auto-sync-left">
        <n-icon size="18" color="#07c160">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M12,18A6,6 0 0,1 6,12C6,11 6.25,10.03 6.7,9.2L5.24,7.74C4.46,8.97 4,10.43 4,12A8,8 0 0,0 12,20V23L16,19L12,15M12,4V1L8,5L12,9V6A6,6 0 0,1 18,12C18,13 17.75,13.97 17.3,14.8L18.76,16.26C19.54,15.03 20,13.57 20,12A8,8 0 0,0 12,4Z"/>
          </svg>
        </n-icon>
        <div class="auto-sync-texts">
          <div class="title">自动同步</div>
          <div class="desc">开启后应用将在空闲时自动执行增量同步。</div>
        </div>
      </div>
      <n-switch :value="value" @update:value="onToggle" size="small">
        <template #checked>开启</template>
        <template #unchecked>关闭</template>
      </n-switch>
    </div>
  </n-card>
</template>

<script setup lang="ts">
import { NCard, NIcon, NSwitch } from 'naive-ui'

defineProps<{ value: boolean }>();
const emit = defineEmits<{ (e: 'update:value', v: boolean): void }>();

const onToggle = (v: boolean) => emit('update:value', v);
</script>

<style scoped>
.auto-sync-card { margin-bottom: 16px; }
.auto-sync-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.auto-sync-left { display: flex; align-items: center; gap: 10px; }
.auto-sync-texts .title { font-size: 14px; color: #333; font-weight: 500; }
.auto-sync-texts .desc { font-size: 12px; color: #888; }
</style>
