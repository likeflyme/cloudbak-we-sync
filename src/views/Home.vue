<template>
  <n-layout class="h-screen">
    <!-- 顶部头部栏（覆盖侧边栏与内容区域） -->
    <n-layout-header class="app-header">
      <div class="left">
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
      </div>
      <div class="right">
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

    <!-- 更新提示气泡 -->
    <div v-if="showUpdateToast" class="update-toast" @click="goUpdate" title="点击查看更新详情">
      <span>发现新版本 v{{ updateVersion }}</span>
      <button class="close" @click.stop="dismissUpdateToast">×</button>
    </div>

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
        <!-- 移除更新面板 -->
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

    <!-- 关于我们弹窗 -->
    <n-modal v-model:show="showAboutDialog" preset="card" title="关于我们" style="max-width:420px;">
      <div style="font-size:14px; line-height:1.6;">
        <p><strong>客户端版本号：</strong>{{ clientVersion }}</p>
        <p><strong>官方网站：</strong><a href="https://www.cloudbak.org" target="_blank">https://www.cloudbak.org</a></p>
        <p><strong>社区论坛：</strong><a href="https://forum.cloudbak.org.cn" target="_blank">https://forum.cloudbak.org.cn</a></p>
      </div>
    </n-modal>
  </n-layout>
</template>

<script setup lang="ts">
import { ref, provide, computed, onMounted } from 'vue'
import { NLayout, NLayoutContent, NLayoutHeader, NButton, NIcon, NDropdown, NModal } from 'naive-ui'
import { invoke } from '@tauri-apps/api/core'
import { removeToken } from '@/common/login'
import { useRouter } from 'vue-router'
import { getSessions, addSession } from '@/api/user'
import { token as getToken, endpoint } from '@/common/login'
import { getVersion } from '@tauri-apps/api/app'
// 临时类型声明避免 TS 报错（若 @tauri-apps/plugin-updater 未提供类型）
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import { check } from '@tauri-apps/plugin-updater'

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
const showAboutDialog = ref(false)
const appVersion = ref<string>('未知')

const menuOptions = [
  { label: '系统设置', key: 'settings' },
  { label: '检查更新', key: 'update' },
  { label: '关于我们', key: 'about' },
  { label: '退出登录', key: 'logout' }
]

onMounted(async () => {
  try { 
    appVersion.value = await getVersion() 
  } catch(e) {
    console.error('获取应用版本失败:', e)
  }
})

// 计算客户端版本号（关于我们用软件版本号）
const clientVersion = computed(() => appVersion.value)

const onMenuSelect = (key: string) => {
  if (key === 'settings') {
    router.push({ name: 'Settings' })
  } else if (key === 'update') {
    router.push({ name: 'UpdateDetail' })
  } else if (key === 'about') {
    showAboutDialog.value = true
  } else if (key === 'logout') {
    const ok = window.confirm('确定要退出登录吗？')
    if (!ok) return
    removeToken()
    try { invoke('clear_auth_context') } catch {}
    router.push('/login')
  }
}

// 已移除首页更新检查逻辑

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
    console.log('新增会话：', newSession)
    addSession(newSession)
      .then((resp) => {
        console.log(resp)
        sessions.value.push(resp)
        newSessionData.value = null
        router.push({ name: 'SessionDetail', params: { id: resp.id } })
      })
      .catch((error) => {
        console.error('Error adding session:', error)
      })
  }
}

const loadSessions = () => {
  getSessions().then((data) => {
    if (data) {
      for (const d of data as Session[]) {
        sessions.value.push(d)
      }
    }
    // initialize auto sync watchers for sessions marked auto_sync
    try {
      const userId = Number(localStorage.getItem('user_id') || '0')
      if (userId > 0) {
        const baseUrl = endpoint() + '/api'
        const t = getToken() || undefined
        invoke('init_user_auto_sync', { userId, baseUrl, token: t }).catch(() => {})
      }
    } catch {}
  })
}

// 更新提示相关状态
const showUpdateToast = ref(false)
const updateVersion = ref('')

const goUpdate = () => {
  showUpdateToast.value = false
  router.push({ name: 'UpdateDetail' })
}
const dismissUpdateToast = () => {
  showUpdateToast.value = false
}

onMounted(async () => {
  try {
    const result = await check()
    console.log('更新检查结果:', result)
    // 兼容不同返回结构：有些版本返回 { available: boolean, version: string }
    if (result) {
      const available = (result as any).available ?? true // 旧版若有返回对象即表示可用
      console.log('更新可用:', available)
      const ver = (result as any).version || (result as any).manifestVersion || ''
      console.log('更新版本:', ver)
      if (available) {
        updateVersion.value = ver || '未知'
        showUpdateToast.value = true
        console.log('显示更新提示:', showUpdateToast.value)
      }
    }
  } catch (e) {
    // 静默失败，不影响使用
    console.warn('更新检查失败:', e)
  }
})

// 提供删除方法给子路由页面调用（如 SessionDetailPage）
const removeSessionById = (id: number) => {
  const idx = sessions.value.findIndex((s) => s.id === id)
  if (idx !== -1) sessions.value.splice(idx, 1)
  if (selected.value?.id === id) selected.value = null
}
provide('removeSessionById', removeSessionById)

loadSessions()
</script>

<style scoped>
.h-screen { height: 100vh; background: #f7f7f7; }
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
.app-header .left, .app-header .right { display: flex; align-items: center; }
.app-header .left { gap: 8px; }
.app-header .logo { width: 20px; height: 20px; }
.app-header .title { font-size: 14px; font-weight: 600; color: #333; }
.body-layout { height: calc(100vh - 50px); overflow: hidden; }
.main-content { height: 100%; overflow: auto; background: #f7f7f7; }
.update-toast {
  position: fixed; /* 使用 fixed 保证相对窗口定位 */
  top: 8px;
  right: 70px;
  background: #fffbe6;
  border: 1px solid #ffecb3;
  padding: 6px 10px;
  font-size: 12px;
  border-radius: 4px;
  box-shadow: 0 2px 6px rgba(0,0,0,0.12);
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  z-index: 1000; /* 比 header (z-index:10) 更高 */
}
.update-toast .close {
  background: transparent;
  border: none;
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  color: #999;
}
.update-toast .close:hover { color: #666; }
.update-panel {
  background: #fff;
  border: 1px solid #e5e5e5;
  padding: 12px 16px;
  border-radius: 6px;
  margin-bottom: 16px;
  box-shadow: 0 2px 6px rgba(0,0,0,0.08);
}
.update-panel h3 { margin: 0 0 8px; font-size: 14px; }
.update-panel .notes { background: #f8f8f8; padding: 8px; white-space: pre-wrap; font-size: 12px; border-radius: 4px; }
.update-panel .actions { display: flex; gap: 8px; margin-top: 8px; }

.update-toast {
  position: fixed; /* 使用 fixed 保证相对窗口定位 */
  top: 8px;
  right: 70px;
  background: #fffbe6;
  border: 1px solid #ffecb3;
  padding: 6px 10px;
  font-size: 12px;
  border-radius: 4px;
  box-shadow: 0 2px 6px rgba(0,0,0,0.12);
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  z-index: 1000; /* 比 header (z-index:10) 更高 */
}
.update-toast .close {
  background: transparent;
  border: none;
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  color: #999;
}
.update-toast .close:hover { color: #666; }
</style>
