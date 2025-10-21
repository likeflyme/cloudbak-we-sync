<template>
  <!-- 使用卡片统一视觉风格 -->
  <div class="update-wrapper">
    <n-card class="update-card" title="客户端更新" size="small" :bordered="false">
        <div v-if="loading" class="status-row">
        <n-spin size="small" /> <span class="status-text">正在检查更新...</span>
        </div>
        <div v-else-if="!info" class="status-row">
        <n-icon size="18" class="ok-icon">
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2a10 10 0 1 0 10 10A10.011 10.011 0 0 0 12 2Zm-1 15-5-5 1.414-1.414L11 14.172l6.586-6.586L19 9l-8 8Z"/></svg>
        </n-icon>
        <span class="status-text">当前已是最新版本</span>
        <!-- <n-button size="tiny" quaternary @click="reCheck">重新检查</n-button> -->
        </div>
        <div v-else>
        <div class="version-row">
            <n-tag type="success" size="small">新版本 {{ info.version }}</n-tag>
            <!-- <n-button size="tiny" quaternary @click="reCheck" :disabled="updating">重新检查</n-button> -->
        </div>
        <div class="notes-wrap">
            <div class="notes-title">更新说明</div>
            <n-scrollbar class="notes-scroll">
            <pre class="notes">{{ info.notes || '无说明' }}</pre>
            </n-scrollbar>
        </div>
        <div class="actions">
            <n-button type="primary" size="small" @click="doUpdate" :loading="updating">立即更新</n-button>
        </div>
        </div>
    </n-card>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NCard, NButton, NSpin, NTag, NScrollbar, NIcon } from 'naive-ui'
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import { check } from '@tauri-apps/plugin-updater'

interface UpdateInfo { version: string; notes?: string }
const info = ref<UpdateInfo | null>(null)
const updateResult = ref<any | null>(null)
const loading = ref(true)
const updating = ref(false)

const runCheck = async () => {
  loading.value = true
  info.value = null
  updateResult.value = null
  try {
    const result = await check()
    if (result?.available) {
      const notes = (result as any).rawJson?.notes || (result as any).body || ''
      info.value = { version: result.version, notes }
      updateResult.value = result
    }
  } catch (e) {
    console.warn('更新检查失败', e)
  } finally {
    loading.value = false
  }
}

const doUpdate = async () => {
  updating.value = true
  try {
    // 重新检查确保对象最新
    const latest = await check()
    console.log(latest);
    if (!latest?.available) { alert('未发现可用更新'); return }
    await latest.download()
    await latest.install()
  } catch (e) {
    console.error('安装更新失败', e)
    alert('安装更新失败: ' + ((e as any)?.message || String(e)))
  } finally {
    updating.value = false
  }
}

// const reCheck = () => runCheck() // 已在模板注释，如需启用重新检查按钮可取消注释

onMounted(runCheck)
</script>

<style scoped>
.update-wrapper { padding: 20px;}
.update-card { padding: 4px; }
.status-row { display: flex; align-items: center; gap: 8px; font-size: 13px; }
.status-text { color: #555; }
.ok-icon { color: #07c160; }
.version-row { display: flex; align-items: center; gap: 10px; margin-bottom: 12px; }
.notes-wrap { border: 1px solid #eee; background: #fafafa; border-radius: 4px; }
.notes-title { font-size: 12px; padding: 6px 10px; border-bottom: 1px solid #eee; color: #666; }
.notes-scroll { max-height: 160px; }
.notes { margin: 0; padding: 10px 12px; font-size: 12px; line-height: 1.5; white-space: pre-wrap; }
.actions { margin-top: 12px; display: flex; gap: 12px; }
</style>
