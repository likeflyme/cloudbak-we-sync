<template>
  <div class="settings-wrapper">
    <n-card title="系统设置" class="settings-card">
      <div class="setting-item">
        <div class="label">占位设置</div>
        <div class="value">后续可在此添加全局选项</div>
      </div>
    </n-card>
  </div>
</template>

<script setup lang="ts">
import { NCard } from 'naive-ui'
</script>

<style scoped>
.settings-wrapper { padding: 20px; }
.settings-card { max-width: 900px; margin: 0 auto; }
.setting-item { display: flex; justify-content: space-between; padding: 8px 0; color: #555; }
.label { font-weight: 500; }
</style>
