<template>
  <div class="settings-wrapper">
    <n-card title="系统设置" class="settings-card">
      <div class="setting-item">
        <div class="label">开机自启</div>
        <div class="value">
          <n-switch :value="autoLaunch" @update:value="onToggleAutoLaunch" />
        </div>
      </div>
      <div class="setting-item">
        <div class="label">当前启动方式</div>
        <div class="value">
          <n-tag size="small" :type="wasAuto ? 'success' : 'info'">
            {{ wasAuto ? '自启后台运行' : '用户手动启动' }}
          </n-tag>
        </div>
      </div>
      <div class="setting-item">
        <div class="label">说明</div>
        <div class="value explain">设置为开机自启后，系统启动时不会显示 UI，需手动打开或从托盘唤起。</div>
      </div>
    </n-card>
  </div>
</template>

<script setup lang="ts">
import { NCard, NSwitch, NTag, createDiscreteApi } from 'naive-ui'
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const autoLaunch = ref(false)
const wasAuto = ref(false)
const { message } = createDiscreteApi(['message'])

const loadStatus = async () => {
  try {
    // 使用插件指令查询是否已启用自启动
    autoLaunch.value = await invoke<boolean>('plugin:autostart|is_enabled')
  } catch (e) {
    console.warn('plugin:autostart|is_enabled failed', e)
  }
  try {
    wasAuto.value = await invoke<boolean>('was_auto_launched')
  } catch (e) {
    console.warn('was_auto_launched failed', e)
  }
}

const onToggleAutoLaunch = async (val: boolean) => {
  try {
    if (val) {
      await invoke('plugin:autostart|enable')
    } else {
      await invoke('plugin:autostart|disable')
    }
    autoLaunch.value = val
    message.success(val ? '已开启开机自启' : '已关闭开机自启')
  } catch (e) {
    console.warn('toggle autostart failed', e)
    message.error('操作失败')
  }
}

onMounted(loadStatus)
</script>

<style scoped>
.settings-wrapper { padding: 20px; }
.settings-card { max-width: 900px; margin: 0 auto; }
.setting-item { display: flex; justify-content: space-between; padding: 10px 0; align-items: center; color: #555; border-bottom: 1px solid #f0f0f0; }
.setting-item:last-child { border-bottom: none; }
.label { font-weight: 500; font-size: 14px; }
.value { display: flex; align-items: center; gap: 8px; }
.explain { max-width: 560px; font-size: 12px; color: #777; line-height: 1.4; }
</style>
