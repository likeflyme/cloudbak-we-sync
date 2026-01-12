<template>
  <div class="settings-wrapper">
    <n-card title="系统信息" class="settings-card">
      <div class="setting-item">
        <div class="label">客户端版本</div>
        <div class="value">
          {{ clientVersion }}
        </div>
      </div>
      <div class="setting-item">
        <div class="label">服务端地址</div>
        <div class="value">
          {{ endpoint }}
        </div>
      </div>
      <div class="setting-item">
        <div class="label">服务端版本</div>
        <div class="value">
          {{ sysInfo?.sys_version || 'unknown' }}
        </div>
      </div>
      <div class="setting-item">
        <div class="label">服务端初始化时间</div>
        <div class="value">
          {{ formatDate(sysInfo?.install) }}
        </div>
      </div>
      <div class="setting-item">
        <div class="label">系统唯一标识</div>
        <div class="value">
          {{ sysInfo?.client_id || '-' }}
        </div>
      </div>
      <div class="setting-item">
        <div class="label">授权码</div>
        <div class="value">
          {{ sysInfo?.license || '-' }}
        </div>
      </div>
    </n-card>
  </div>
</template>

<script setup lang="ts">
import { NCard } from 'naive-ui'
import { ref, onMounted, computed } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { getEndpointFromStore, getSysInfoFromStore } from '@/common/store'

const endpoint = ref<string>('')

const appVersion = ref<string>('未知')
const sysInfo = ref<any | null>(null)

onMounted(async () => {
  try { 
    appVersion.value = await getVersion() 
  } catch(e) {
    console.error('获取应用版本失败:', e)
  }
  await loadFromStore()
})

// 计算客户端版本号（关于我们用软件版本号）
const clientVersion = computed(() => appVersion.value)

const loadFromStore = async () => {
  try {
    sysInfo.value = await getSysInfoFromStore<any>()
    const ep = await getEndpointFromStore()
    endpoint.value = ep || localStorage.getItem('endpoint') || ''
  } catch (e) {
    console.warn('load sys_info/endpoint from store failed', e)
  }
}

const formatDate = (v: any) => {
  if (!v) return '-'
  try {
    // 支持 ISO 字符串或时间戳
    const d = typeof v === 'string' ? new Date(v) : new Date(Number(v))
    if (isNaN(d.getTime())) return String(v)
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  } catch { return String(v) }
}
</script>

<style scoped>
.settings-wrapper { padding: 20px; }
.settings-card { max-width: 900px; margin: 0 auto; }
.setting-item { display: flex; justify-content: space-between; padding: 10px 0; align-items: flex-start; color: #555; border-bottom: 1px solid #f0f0f0; }
.setting-item:last-child { border-bottom: none; }
.label { font-weight: 500; font-size: 14px; flex: 0 0 160px; width: 160px; white-space: nowrap; }
.value { flex: 1; min-width: 0; overflow-wrap: anywhere; word-break: break-word; }
.value pre { white-space: pre-wrap; word-break: break-word; }
.explain { max-width: 560px; font-size: 12px; color: #777; line-height: 1.4; }
</style>
