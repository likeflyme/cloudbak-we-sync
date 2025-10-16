<template>
  <n-layout has-sider class="h-screen">
    <!-- 左侧会话列表 -->
    <SessionSidebar 
      :sessions="sessions"
      :selected-id="selected?.id"
      @select-session="selectSession"
      @add-session="showAddDialog"
      @logout="logout"
    />

    <!-- 右侧详情区域改为路由视图 -->
    <n-layout-content class="main-content">
      <LoadingState v-if="isAddingSession" />
      <NewSessionPreview 
        v-else-if="newSessionData" 
        :session-data="newSessionData"
        @confirm="confirmAdd"
        @cancel="cancelAdd"
      />
      <router-view v-else />
    </n-layout-content>
  </n-layout>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { NLayout, NLayoutContent } from 'naive-ui'
import { invoke } from '@tauri-apps/api/core'
import { removeToken } from '@/common/login'
import { useRouter } from 'vue-router'

import { getSessions, addSession } from '@/api/user'

// 导入组件
import SessionSidebar from '@/components/Session/SessionSidebar.vue'
import NewSessionPreview from '@/components/SessionDetail/NewSessionPreview.vue'
import LoadingState from '@/components/SessionDetail/LoadingState.vue'
import type { Session, PartialSession } from '@/models/session'

const router = useRouter()

// 实际应从 API 获取
const sessions = ref<Session[]>([])

const selected = ref<Session | null>(null)
const isAddingSession = ref(false)
const newSessionData = ref<PartialSession | null>(null)

const selectSession = (s: Session) => {
  selected.value = s
  newSessionData.value = null
  router.push({ name: 'SessionDetail', params: { id: s.id } })
}

// 显示添加对话框 -> 确认后调用后端提取并创建会话
const showAddDialog = async () => {
  selected.value = null

  const ok = window.confirm('是否开始扫描并添加微信会话？\n请确保已登录且微信 v4 正在运行。')
  if (!ok) return

  isAddingSession.value = true
  try {
    const res: any = await invoke('extract_wechat_keys', { dataDir: null })
    if (res?.ok) {
      const account = res.accountName || '未知账号'

      let dataDir = res.dataDir as string | null
      if (dataDir && dataDir.startsWith('\\\\?\\')) {
        dataDir = dataDir.slice(4)
      }

      const clientType = res.clientType || 'win'
      const clientVersion = res.clientVersion || ''

      // 初始化新会话数据（使用新字段名，并填充旧字段以兼容现有组件）
      const draft: PartialSession = {
        name: '',
        desc: '',
        wx_id: account || '',
        wx_acct_name: '',
        wx_mobile: '',
        wx_email: '',
        wx_dir: dataDir || '',
        avatar: '',
        wx_key: res.dataKey || '',
        aes_key: res.imageKey || '',
        xor_key: res.xorKey != null ? String(res.xorKey) : '',
        client_type: clientType,
        client_version: clientVersion,
        // legacy aliases for compatibility
        wx_name: '',
        data_key: res.dataKey || ''
      }

      // 如果后端提供了本地头像路径，解析为可用的 data/url
      if (res.headImg) {
        try {
          const avatarData: string = await invoke('load_avatar', { path: res.headImg })
          if (avatarData) {
            draft.avatar = avatarData
          }
        } catch (e) {
          console.warn('load_avatar 调用失败:', e)
        }
      }

      newSessionData.value = draft
    } else {
      const msg = res?.error || '提取失败，未返回可用数据'
      alert(msg)
    }
  } catch (e: any) {
    console.error('extract_wechat_keys 调用失败:', e)
    alert(`调用失败: ${e?.message || String(e)}`)
  } finally {
    isAddingSession.value = false
  }
}

// 取消添加（返回列表，不写入）
const cancelAdd = () => {
  newSessionData.value = null
}

// 确认添加：从预览/编辑页把最终信息加入会话列表
const confirmAdd = (sessionData: PartialSession) => {
  if (sessionData) {
    const newSession: Session = {
      id: sessions.value.length + 1,
      // 保留用户在预览页修改后的所有字段
      ...sessionData,
      // 兼容旧字段（如果预览页只设置了新字段，补齐旧别名）
      wx_name: sessionData.wx_name ?? (sessionData as any).wx_acct_name ?? '',
      data_key: sessionData.data_key ?? (sessionData as any).wx_key ?? '',
      // 一些兜底字段
      online: (sessionData as any).online ?? true,
      lastActive: (sessionData as any).lastActive ?? '刚刚',
      autoSync: (sessionData as any).autoSync ?? false,
      syncFilters: (sessionData as any).syncFilters ?? ''
    } as Session
    addSession(newSession).then((resp) => {
      console.log(resp);
      sessions.value.push(newSession)
      selected.value = newSession
      newSessionData.value = null
      router.push({ name: 'SessionDetail', params: { id: newSession.id } })
    });
  }
}

const logout = () => {
  const ok = window.confirm('确定要退出登录吗？');
  if (!ok) return;
  removeToken();
  router.push('/login');
}

const loadSessions = () => {
  getSessions().then((data) => {
    if (data) {
      for (const d of data as Session[]) {
        sessions.value.push(d)
      }
    }
  })
}

loadSessions()
</script>

<style scoped>
.h-screen {
  height: 100vh;
  background: #f7f7f7;
}

.main-content {
  background: #f7f7f7;
}
</style>
