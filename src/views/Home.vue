<template>
  <n-layout class="h-screen">
    <!-- 顶部头部栏（覆盖侧边栏与内容区域） -->
    <n-layout-header class="app-header">
      <div class="left">
        <img class="logo" src="/vite.svg" alt="logo" />
        <span class="title">We Sync</span>
      </div>
      <div class="right">
        <!-- Add Session button placed left to settings dropdown -->
        <n-button quaternary circle @click="showAddDialog" style="margin-right: 6px;" title="添加会话">
          <template #icon>
            <n-icon>
              <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M19,13H13V19H11V13H5V11H11V5H13V11H19V13Z"/>
              </svg>
            </n-icon>
          </template>
        </n-button>
        <!-- Open endpoint in browser -->
         <!--
        <n-button quaternary circle @click="openEndpoint" style="margin-right: 8px;" title="打开服务器">
          <template #icon>
            <n-icon>
              <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M14,3H21V10H19V5.41L10.41,14L9,12.59L17.59,4H14V3M5,5H12V7H7V17H17V12H19V19A2,2 0 0,1 17,21H7A2,2 0 0,1 5,19V5Z"/>
              </svg>
            </n-icon>
          </template>
        </n-button>
        -->
        <n-dropdown :options="menuOptions" trigger="click" @select="onMenuSelect">
          <n-button quaternary circle>
            <template #icon>
              <n-icon>
                <!-- simplified three-dots icon -->
                <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <circle cx="6" cy="12" r="2" />
                  <circle cx="12" cy="12" r="2" />
                  <circle cx="18" cy="12" r="2" />
                </svg>
              </n-icon>
            </template>
          </n-button>
        </n-dropdown>
      </div>
    </n-layout-header>

    <!-- 下方主体：左侧侧边栏 + 右侧内容区域 -->
    <n-layout has-sider class="body-layout">
      <!-- 左侧会话列表（侧边栏） -->
      <SessionSidebar 
        :sessions="sessions"
        :selected-id="selected?.id"
        @select-session="selectSession"
      />

      <!-- 右侧内容区域 -->
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
  </n-layout>
</template>

<script setup lang="ts">
import { ref, provide } from 'vue'
import { NLayout, NLayoutContent, NLayoutHeader, NButton, NIcon, NDropdown } from 'naive-ui'
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

const menuOptions = [
  { label: '系统设置', key: 'settings' },
  { label: '退出登录', key: 'logout' }
]

const onMenuSelect = (key: string) => {
  if (key === 'settings') {
    router.push({ name: 'Settings' })
  } else if (key === 'logout') {
    const ok = window.confirm('确定要退出登录吗？')
    if (!ok) return
    removeToken()
    router.push('/login')
  }
}

const openEndpoint = async () => {
  const url = (localStorage.getItem('endpoint') || '').trim()
  if (!url) {
    alert('未配置服务器地址');
    return;
  }
  try {
    const u = new URL(url)
    if (!['http:', 'https:'].includes(u.protocol)) throw new Error('invalid')
    const mod: any = await import('@tauri-apps/plugin-opener')
    if (mod?.open) {
      await mod.open(url)
    } else if (mod?.default) {
      await mod.default(url)
    } else {
      window.open(url, '_blank')
    }
  } catch {
    alert('服务器地址无效')
  }
}

const selectSession = (s: Session) => {
  selected.value = s
  newSessionData.value = null
  router.push({ name: 'SessionDetail', params: { id: s.id } })
}

// 提供一个 getter，避免子页重复请求列表
const getSessionById = (id: number) => sessions.value.find(s => s.id === id) || null
provide('getSessionById', getSessionById)

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
      // 保留用户在预览页修改后的所有字段
      ...sessionData,
    } as Session
    console.log("新增会话：", newSession);
    addSession(newSession).then((resp) => {
      console.log(resp);
      sessions.value.push(resp)
      newSessionData.value = null
      router.push({ name: 'SessionDetail', params: { id: resp.id } })
    }).catch((error) => {
      console.error('Error adding session:', error)
    });
  }
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

// 提供删除方法给子路由页面调用（如 SessionDetailPage）
const removeSessionById = (id: number) => {
  const idx = sessions.value.findIndex(s => s.id === id)
  if (idx !== -1) sessions.value.splice(idx, 1)
  if (selected.value?.id === id) selected.value = null
}
provide('removeSessionById', removeSessionById)

loadSessions()
</script>

<style scoped>
.h-screen {
  height: 100vh;
  background: #f7f7f7;
}

.app-header {
  position: sticky;
  top: 0;
  z-index: 10;
  height: 50px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px 0 16px;
  background: #fff;
  border-bottom: 1px solid #eee;
}
.app-header .left { display: flex; align-items: center; gap: 8px; }
.app-header .logo { width: 20px; height: 20px; }
.app-header .title { font-size: 14px; font-weight: 600; color: #333; }

/* 主体布局高度为视口高度减去头部，且不让容器滚动 */
.body-layout {
  height: calc(100vh - 50px);
  overflow: hidden;
}

/* 让右侧内容区域拥有独立滚动条 */
.main-content {
  height: 100%;
  overflow: auto;
  background: #f7f7f7;
}
</style>
