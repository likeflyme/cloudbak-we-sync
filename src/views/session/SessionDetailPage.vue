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
          @toggle-auto-sync="toggleAutoSync"
          @toggle-key-visibility="toggleKeyVisibility"
          @copy-key="copyKey"
          @update:syncFilters="updateSyncFilters"
          @sync="handleSync"
          @delete="deleteSession"
        />
      </template>
      <template v-else>
        <EmptyState />
      </template>
    </template>
  </n-layout-content>
</template>

<script setup lang="ts">
import { ref, onMounted, watch, inject } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { NLayoutContent } from 'naive-ui'
import LoadingState from '@/components/SessionDetail/LoadingState.vue'
import EmptyState from '@/components/SessionDetail/EmptyState.vue'
import SessionDetail from '@/views/session/SessionDetail.vue'
import { getSessions } from '@/api/user'
import type { Session } from '@/models/session'
import { deleteSession as deleteSessionFromServer } from '@/api/user'

const route = useRoute()
const router = useRouter()
const loading = ref(true)
const session = ref<Session | null>(null)
const keyVisibility = ref({
  data_key: false,
  aes_key: false,
  xor_key: false
})

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
    const list = (await getSessions()) as any[]
    const found = list.find(s => s.id === id)
    session.value = found ? normalize(found) : null
  } finally {
    loading.value = false
  }
}

onMounted(load)
watch(() => route.params.id, load)

const toggleAutoSync = () => {
  if (!session.value) return
  session.value.autoSync = !session.value.autoSync
}

const toggleKeyVisibility = (key: 'data_key' | 'aes_key' | 'xor_key') => {
  keyVisibility.value[key] = !keyVisibility.value[key]
}

const updateSyncFilters = (value: string) => {
  if (!value || !session.value) return
  session.value.syncFilters = value
}

const handleSync = (key: string) => {
  if (!session.value) return
  console.log('对会话', session.value.id, '执行操作:', key)
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
  deleteSessionFromServer(id).then(() => {
    removeSessionById && removeSessionById(id)
    router.push('/')
  }).catch((error) => {
    console.error('Error deleting session:', error)
  });
}
</script>

<style scoped>
.main-content { background: #f7f7f7; }
</style>
