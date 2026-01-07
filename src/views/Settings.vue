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

      <!-- 新增：本地解析开关 -->
      <!-- 
      <div class="setting-item">
        <div class="label">本地解析</div>
        <div class="value">
          <n-switch :value="localParse" @update:value="onToggleLocalParse" />
        </div>
      </div>
      <div class="setting-item">
        <div class="label">说明</div>
        <div class="value explain">使用当前计算机做数据文件解析</div>
      </div>
      -->
    </n-card>
  </div>
</template>

<script setup lang="ts">
import { NCard, NSwitch, NTag, createDiscreteApi } from 'naive-ui'
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Store } from '@tauri-apps/plugin-store'

const autoLaunch = ref(false)
const wasAuto = ref(false)
const localParse = ref(false)
const { message } = createDiscreteApi(['message'])

const LOCAL_PARSE_KEY = 'local_parse_enabled'
let settingsStore: Store | null = null

const initStore = async () => {
  try {
    // 使用 v2 API 静态方法加载并返回实例
    settingsStore = await Store.load('settings.json')
  } catch (e) {
    console.warn('store init failed', e)
    settingsStore = null
  }
}

const loadStatus = async () => {
  try {
    autoLaunch.value = await invoke<boolean>('plugin:autostart|is_enabled')
  } catch (e) {
    console.warn('plugin:autostart|is_enabled failed', e)
  }
  try {
    wasAuto.value = await invoke<boolean>('was_auto_launched')
  } catch (e) {
    console.warn('was_auto_launched failed', e)
  }
  try {
    if (!settingsStore) await initStore()
    const v = (await settingsStore?.get<boolean>(LOCAL_PARSE_KEY)) ?? false
    localParse.value = !!v
  } catch (e) {
    console.warn('load local parse from store failed', e)
  }
  appDataPath.value = await appDataDir()
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

const onToggleLocalParse = async (val: boolean) => {
  try {
    if (!settingsStore) await initStore()
    await settingsStore?.set(LOCAL_PARSE_KEY, val)
    await settingsStore?.save()
    localParse.value = val
    message.success(val ? '已开启本地解析' : '已关闭本地解析')
  } catch (e) {
    console.warn('toggle local parse failed', e)
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
