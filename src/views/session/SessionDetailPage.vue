<template>
  <n-layout-content class="main-content">
    <template v-if="loading">
      <LoadingState />
    </template>
    <template v-else>
      <template v-if="session">
        <SessionDetail
          :session="session"
          :key-visibility="keyVisibility"
          :syncing="isRunning"
          @toggle-auto-sync="toggleAutoSync"
          @toggle-key-visibility="toggleKeyVisibility"
          @copy-key="copyKey"
          @update:syncFilters="updateSyncFilters"
          @sync="handleSync"
          @delete="deleteSession"
        />

        <!-- Sync status panel -->
        <div class="sync-status" v-if="syncStatus.state !== 'idle'">
          <!-- changed: add vertical prop to n-space to enforce column layout -->
          <n-space vertical class="sync-status-inner">
            <div class="status-text">
              <n-alert :type="statusType" :title="statusTitle" :show-icon="true" class="status-alert">
                <div>
                  <div v-if="syncStatus.message" class="status-message">{{ syncStatus.message }}</div>
                  <div class="stats">
                    <span>扫描: {{ syncStatus.scanned }}</span>
                    <span>待传: {{ syncStatus.to_upload }}</span>
                    <span>已传: {{ syncStatus.uploaded }}</span>
                    <span>跳过: {{ syncStatus.skipped }}</span>
                    <span>失败: {{ syncStatus.failed }}</span>
                    <span v-if="syncStatus.current" class="current" :title="syncStatus.current">当前: {{ syncStatus.current }}</span>
                  </div>
                </div>
              </n-alert>
            </div>
            <div class="status-actions">
              <n-button v-if="isRunning" type="warning" @click="stopSync">停止同步</n-button>
            </div>
          </n-space>
        </div>
      </template>
      <template v-else>
        <EmptyState />
      </template>
    </template>
  </n-layout-content>
</template>

<script setup lang="ts">
import { ref, onMounted, watch, inject, computed, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { NLayoutContent, NAlert, NButton, NSpace, createDiscreteApi } from 'naive-ui'
import LoadingState from '@/components/SessionDetail/LoadingState.vue'
import EmptyState from '@/components/SessionDetail/EmptyState.vue'
import SessionDetail from '@/views/session/SessionDetail.vue'
import type { Session } from '@/models/session'
import { deleteSession as deleteSessionFromServer } from '@/api/user'
import { invoke } from '@tauri-apps/api/core'
import { endpoint, token as getToken } from '@/common/login'
import { decrypt } from '@/api/task' // 新增: 同步完成后调用解密任务

const userId = Number(localStorage.getItem('user_id') || '0')

const route = useRoute()
const router = useRouter()
const loading = ref(true)
const session = ref<Session | null>(null)
const keyVisibility = ref({
  data_key: false,
  aes_key: false,
  xor_key: false
})

const { message } = createDiscreteApi(['message'])

// get session by id from parent (Home.vue)
const getSessionById = inject<(id: number) => Session | null | undefined>('getSessionById')

// sync states
const syncing = ref(false)
const taskId = ref<string | null>(null)
const syncStatus = ref<{ state: string; scanned: number; to_upload: number; uploaded: number; skipped: number; failed: number; current?: string; message?: string}>({
  state: 'idle', scanned: 0, to_upload: 0, uploaded: 0, skipped: 0, failed: 0
})
let pollTimer: number | null = null
const manualSyncInProgress = ref(false) // 新增: 标记本次是否为手动同步

const isRunning = computed(() => syncStatus.value.state === 'running')
const statusType = computed(() => {
  switch (syncStatus.value.state) {
    case 'running': return 'info'
    case 'done': return 'success'
    case 'stopped': return 'warning'
    case 'error': return 'error'
    default: return 'default'
  }
})
const statusTitle = computed(() => {
  switch (syncStatus.value.state) {
    case 'running': return '正在同步'
    case 'done': return '同步完成'
    case 'stopped': return '已停止'
    case 'error': return '同步出错'
    default: return '同步状态'
  }
})

const startPolling = () => {
  stopPolling()
  pollTimer = window.setInterval(async () => {
    if (!taskId.value) return
    try {
      const st = await invoke<any>('get_sync_status', { taskId: taskId.value })
      syncStatus.value = st
      if (st.state === 'done' || st.state === 'stopped' || st.state === 'error') {
        syncing.value = false
        stopPolling()
        // 新增: 若为手动同步且成功完成则调用后端解析
        if (st.state === 'done' && manualSyncInProgress.value && session.value) {
          try {
            await decrypt(session.value.id)
            message.success('解析任务已启动')
          } catch (e: any) {
            console.error('decrypt error', e)
            message.error(e?.message || '解析任务启动失败')
          } finally {
            manualSyncInProgress.value = false
          }
        } else if (st.state !== 'running') {
          manualSyncInProgress.value = false
        }
      }
    } catch (e) {
      // stop polling on error
      stopPolling()
      syncing.value = false
      manualSyncInProgress.value = false
    }
  }, 600)
}

const stopPolling = () => {
  if (pollTimer != null) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

onUnmounted(stopPolling)

const normalize = (s: any): Session => {
  return {
    ...s,
    wx_acct_name: s.wx_acct_name ?? s.wx_name ?? '',
    wx_key: s.wx_key ?? s.data_key ?? '',
    data_key: s.data_key ?? s.wx_key ?? '',
    // defaults to satisfy child prop requirements
    syncFilters: s.syncFilters ?? '',
    autoSync: s.autoSync ?? false,
    lastActive: s.lastActive ?? '',
    online: s.online ?? true,
  } as Session
}

const load = async () => {
  loading.value = true
  session.value = null
  const id = Number(route.params.id)
  try {
    // resolve from parent-provided list to avoid refetching
    const found = getSessionById ? getSessionById(id) : null
    session.value = found ? normalize(found) : null

    // fetch and apply local persisted sync filters to override server value
    if (session.value) {
      try {
        const filters = await invoke<string>('get_session_filters', { sessionId: session.value.id, userId })
        if (typeof filters === 'string' && filters.length > 0) {
          session.value.syncFilters = filters
        }
      } catch (e) {
        // ignore if not found or error
      }

      // if still empty -> apply default filters and persist
      if (!session.value.syncFilters) {
        const defaultFilters = ['cache/', 'temp/', '*.db-shm', '*.db-wal'].join('\n')
        session.value.syncFilters = defaultFilters
        try {
          await invoke('save_session_filters', { sessionId: session.value.id, userId, syncFilters: defaultFilters })
        } catch {}
      }

      // fetch auto sync state
      try {
        const auto = await invoke<boolean>('get_auto_sync_state', { sessionId: session.value.id, userId })
        session.value.autoSync = !!auto
      } catch {}

      // persist a snapshot of current session info for convenience
      try {
        const { id, name, desc, wx_id, wx_acct_name, wx_mobile, wx_email, wx_dir, avatar, online, lastActive, wx_key, aes_key, xor_key, client_type, client_version } = session.value
        await invoke('save_session_info', { sessionId: id, userId, info: { id, name, desc, wx_id, wx_acct_name, wx_mobile, wx_email, wx_dir, avatar, online, lastActive, wx_key, aes_key, xor_key, client_type, client_version } })
      } catch {}
    }
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(() => route.params.id, () => { stopPolling(); load() })

const toggleAutoSync = async () => {
  if (!session.value) return
  const s = session.value
  const enabling = !s.autoSync
  const baseUrl = await endpoint() + '/api'
  const t = await getToken() || undefined
  if (enabling) {
    try {
      const data = { sysSessionId: s.id, userId, wxDir: s.wx_dir, baseUrl, token: t }
      console.log(data)
      await invoke('start_auto_sync', data)
      s.autoSync = true
      message.success('已开启自动同步')
    } catch (e: any) {
      const msg = e?.message || String(e) || '开启自动同步失败'
      message.error(msg)
    }
  } else {
    try {
      await invoke('stop_auto_sync', { sysSessionId: s.id, userId })
    } catch {}
    s.autoSync = false
    message.success('已关闭自动同步')
  }
}

const toggleKeyVisibility = (key: 'data_key' | 'aes_key' | 'xor_key') => {
  keyVisibility.value[key] = !keyVisibility.value[key]
}

const updateSyncFilters = (value: string) => {
  if (!value || !session.value) return
  session.value.syncFilters = value
}

const handleSync = async (key: string) => {
  if (!session.value || syncing.value) return
  const full = key === 'full'
  try {
    const baseUrl = endpoint() + '/api'
    const t = getToken() || undefined
    const id = await invoke<string>('start_sync', {
      sysSessionId: session.value.id,
      userId,
      wxDir: session.value.wx_dir,
      baseUrl,
      token: t,
      full,
    })
    taskId.value = id
    syncing.value = true
    manualSyncInProgress.value = true // 新增: 标记为手动同步
    syncStatus.value = { state: 'running', scanned: 0, to_upload: 0, uploaded: 0, skipped: 0, failed: 0 }
    startPolling()
  } catch (e) {
    console.error('start_sync error', e)
  }
}

const stopSync = async () => {
  if (!taskId.value) return
  try {
    await invoke('stop_sync', { taskId: taskId.value })
  } catch (e) {
    // ignore
  }
}

const copyKey = async (key: string) => {
  try {
    await navigator.clipboard.writeText(key)
  } catch (err) {
    try {
      const textArea = document.createElement('textarea')
      textArea.value = key
      document.body.appendChild(textArea)
      textArea.focus()
      textArea.select()
      document.execCommand('copy')
      document.body.removeChild(textArea)
    } catch {
      // ignore
    }
  }
}

const removeSessionById = inject<(id: number) => void>('removeSessionById')

const deleteSession = () => {
  // TODO: 调用后端删除会话，完成后返回列表
  if (session.value?.id == null) return;
  const id = session.value.id
  deleteSessionFromServer(id).then(async () => {
    // 停止此会话的自动同步 & 删除本地会话配置文件
    try { await invoke('stop_auto_sync', { sysSessionId: id, userId }) } catch {}
    try { await invoke('delete_session_config', { sessionId: id, userId }) } catch {}
    removeSessionById && removeSessionById(id)
    router.push('/')
  }).catch((error) => {
    console.error('Error deleting session:', error)
  });
}
</script>

<style scoped>
.main-content { background: #f7f7f7; }
.sync-status { margin: 16px 20px; }
.sync-status-inner { display:block; width:100%; max-width:900px; }
.status-text { width:100%; }
.status-actions { width:100%; text-align:right; margin-top:4px; }
.status-alert { width:100%; box-sizing:border-box; }
.status-message { word-break:break-word; overflow-wrap:anywhere; }
.stats { display:flex; gap:12px; flex-wrap:wrap; margin-top:6px; font-size:12px; color:#666; }
.stats span { white-space:nowrap; }
.stats .current { flex:1 1 100%; white-space:normal; word-break:break-all; overflow-wrap:anywhere; max-width:100%; line-height:1.4; }
@media (max-width:760px){
  .sync-status-inner { max-width:100%; width:100%; display:flex; }
  .status-text, .status-alert { min-width:0; width:100%; }
  .stats .current { flex:1 1 100%; }
}
</style>
