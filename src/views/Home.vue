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
        <!-- 调整：取消按钮放到加载组件内部文字下方，用 a 样式 -->
        <div v-if="isAddingSession" class="extracting-wrap">
          <LoadingState :message="loadingMessage" :logs="extractionLogs" :can-cancel="canCancel" @cancel="cancelExtraction" />
        </div>
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
        <p><strong>官方网站：</strong><a href="https://www.cloudbak.org" target="_blank">https://www.cloudbak.org</a></p>
        <p><strong>社区论坛：</strong><a href="https://forum.cloudbak.org.cn" target="_blank">https://forum.cloudbak.org.cn</a></p>
      </div>
    </n-modal>

    <!-- 数据目录选择弹窗 -->
    <n-modal v-model:show="showDirDialog" preset="card" title="选择微信数据目录" style="max-width:520px;" :mask-closable="false">
      <div style="font-size:14px; line-height:1.8;">
        <template v-if="detectedDirs.length > 0">
          <p style="margin-bottom:8px; color:#666;">检测到以下微信数据目录，请选择一个：</p>
          <n-radio-group v-model:value="selectedDir">
            <n-space vertical>
              <n-radio v-for="dir in detectedDirs" :key="dir" :value="dir" :label="dir" />
            </n-space>
          </n-radio-group>
          <p style="margin-top:8px; color: red;">如果检测到多个数据目录，可以在微信中找一个文件，右键在文件夹中显示可查看目录</p>
        </template>
        <template v-else>
          <p style="margin-bottom:8px; color:#999; size: 10px;">未检测到微信数据目录，请手动输入路径：</p>
          <n-input v-model:value="manualDir" placeholder="例如：D:\xwechat_files\wxid_xxx" clearable />
        </template>
      </div>
      <template #footer>
        <n-space justify="end">
          <n-button @click="onDirDialogCancel">取消</n-button>
          <n-button type="primary" @click="onDirDialogConfirm"
            :disabled="detectedDirs.length > 0 ? !selectedDir : !manualDir.trim()">
            确定
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </n-layout>
</template>

<script setup lang="ts">
import { ref, provide, computed, onMounted } from 'vue'
import { NLayout, NLayoutContent, NLayoutHeader, NButton, NIcon, NDropdown, NModal, NInput, NRadioGroup, NRadio, NSpace, useDialog, useMessage } from 'naive-ui'
import { invoke } from '@tauri-apps/api/core'
import { removeToken } from '@/common/login'
import { useRouter } from 'vue-router'
import { getSessions, addSession } from '@/api/user'
import { getSysInfo } from '@/api/sys'
import { setSysInfoToStore, getSysInfoFromStore, clearStoreExceptEndpoint } from '@/common/store'
import { getVersion } from '@tauri-apps/api/app'
// 临时类型声明避免 TS 报错（若 @tauri-apps/plugin-updater 未提供类型）
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import { check } from '@tauri-apps/plugin-updater'
import { listen } from '@tauri-apps/api/event'

// 导入组件
import SessionSidebar from '@/components/Session/SessionSidebar.vue'
import NewSessionPreview from '@/components/SessionDetail/NewSessionPreview.vue'
import LoadingState from '@/components/SessionDetail/LoadingState.vue'
import type { LogEntry } from '@/components/SessionDetail/LoadingState.vue'
import type { Session, PartialSession } from '@/models/session'

const router = useRouter()
const dialog = useDialog()
const message = useMessage()

/** 用 Naive UI Dialog 模拟 window.confirm，返回 Promise<boolean> */
const nConfirm = (content: string, title = '提示'): Promise<boolean> => {
  return new Promise((resolve) => {
    dialog.warning({
      title,
      content,
      positiveText: '确定',
      negativeText: '取消',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
      onMaskClick: () => resolve(false),
    })
  })
}

// 实际应从 API 获取
const sessions = ref<Session[]>([])

const selected = ref<Session | null>(null)
const isAddingSession = ref(false)
const extractionCancelled = ref(false)
const canCancel = computed(() => isAddingSession.value && !extractionCancelled.value)
const newSessionData = ref<PartialSession | null>(null)
const showAboutDialog = ref(false)
const appVersion = ref<string>('未知')

// 日志与加载消息
const loadingMessage = ref('正在扫描微信数据...')
const extractionLogs = ref<LogEntry[]>([])

const appendLog = (text: string, level: LogEntry['level'] = 'info') => {
  const now = new Date()
  const time = [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map(n => String(n).padStart(2, '0')).join(':')
  extractionLogs.value.push({ time, text, level })
}

const resetLogs = () => {
  extractionLogs.value = []
  loadingMessage.value = '正在扫描微信数据...'
}

// 数据目录选择对话框
const showDirDialog = ref(false)
const detectedDirs = ref<string[]>([])
const selectedDir = ref<string>('')
const manualDir = ref<string>('')
let dirDialogResolve: ((dir: string | null) => void) | null = null

const menuOptions = [
  { label: '系统信息', key: 'sysInfo' },
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
  // 监听托盘显示事件刷新主界面（使用标志防并发重复）
  listen('tray-show', () => {
    if (isLoadingSessions.value) { pendingTrayRefresh = true; return }
    loadSessions()
  })
  // 拉取系统信息并持久化到 store
  try {
    await loadSysInfo()
  } catch {}
})

// 菜单选择处理
const onMenuSelect = async (key: string) => {
  if (key === 'settings') {
    router.push({ name: 'Settings' })
  } else if (key === 'sysInfo') {
    router.push({ name: 'SysInfo' })
  } else if (key === 'update') {
    router.push({ name: 'UpdateDetail' })
  } else if (key === 'about') {
    showAboutDialog.value = true
  } else if (key === 'logout') {
    const ok = await nConfirm('确定要退出登录吗？')
    if (!ok) return
    removeToken()
    try { invoke('clear_auth_context') } catch {}
    // 清空 store 中除 endpoint 外的所有数据
    try { await clearStoreExceptEndpoint() } catch {}
    router.push('/login')
  }
}

// 选择会话
const selectSession = (s: Session) => {
  selected.value = s
  newSessionData.value = null
  router.push({ name: 'SessionDetail', params: { id: s.id } })
}

// 提供一个 getter，避免子页重复请求列表
const getSessionById = (id: number) => sessions.value.find(s => s.id === id) || null
provide('getSessionById', getSessionById)

// 数据目录选择对话框回调
const onDirDialogCancel = () => {
  showDirDialog.value = false
  if (dirDialogResolve) { dirDialogResolve(null); dirDialogResolve = null }
}
const onDirDialogConfirm = () => {
  showDirDialog.value = false
  const dir = detectedDirs.value.length > 0 ? selectedDir.value : manualDir.value.trim()
  if (dirDialogResolve) { dirDialogResolve(dir || null); dirDialogResolve = null }
}

/** 弹出目录选择对话框，返回用户选择的目录或 null（取消） */
const promptDirSelection = (dirs: string[]): Promise<string | null> => {
  return new Promise((resolve) => {
    detectedDirs.value = dirs
    selectedDir.value = dirs.length > 0 ? dirs[0] : ''
    manualDir.value = ''
    dirDialogResolve = resolve
    showDirDialog.value = true
  })
}

const parseSemver = (v: string): [number, number, number] => {
  const s = String(v || '').trim()
  const m = s.match(/(\d+)\.(\d+)\.(\d+)/)
  if (!m) return [0, 0, 0]
  return [Number(m[1]) || 0, Number(m[2]) || 0, Number(m[3]) || 0]
}
const gteSemver = (a: string, b: string) => {
  const [a1, a2, a3] = parseSemver(a)
  const [b1, b2, b3] = parseSemver(b)
  if (a1 !== b1) return a1 > b1
  if (a2 !== b2) return a2 > b2
  return a3 >= b3
}
const MIN_ADD_SERVER_VERSION = '2.1.0'

// 显示添加对话框 -> 确认后调用后端提取并创建会话
const showAddDialog = async () => {
  console.log('开始添加会话')
  selected.value = null

  // 检查服务端版本 >= 2.1.0
  try {
    const sysInfo: any = await getSysInfoFromStore()
    const ver = sysInfo?.sys_version || ''
    if (!ver || !gteSemver(ver, MIN_ADD_SERVER_VERSION)) {
      message.error(`服务端版本需 >= ${MIN_ADD_SERVER_VERSION} 才能添加会话（当前：${ver || 'unknown'}），请升级服务端后重新登录客户端。`)
      return
    }
  } catch {
    message.error('无法获取服务端版本信息，请稍后重试')
    return
  }

  const ok = await nConfirm('是否开始扫描并添加微信会话？\n请确保已登录且微信正在运行。')
  if (!ok) return

  isAddingSession.value = true
  extractionCancelled.value = false
  newSessionData.value = null
  resetLogs()
  try {
    // 第零步：检测微信数据目录
    loadingMessage.value = '正在检测微信数据目录...'
    appendLog('开始检测微信数据目录...')
    const dirRes: any = await invoke('detect_data_dirs')
    if (extractionCancelled.value) { appendLog('用户已取消', 'warn'); return }
    const dirs: string[] = (dirRes?.ok && Array.isArray(dirRes.dirs)) ? dirRes.dirs : []

    if (dirs.length > 0) {
      appendLog(`检测到 ${dirs.length} 个数据目录：`, 'success')
      dirs.forEach((d, i) => appendLog(`  [${i + 1}] ${d}`))
    } else {
      appendLog('未检测到微信数据目录', 'warn')
    }

    let chosenDir: string | null = null
    if (dirs.length === 1) {
      // 只有一个目录，直接使用
      chosenDir = dirs[0]
      appendLog(`自动选择唯一目录: ${chosenDir}`, 'info')
    } else {
      // 多个或零个，弹窗让用户选择或输入
      isAddingSession.value = false // 暂时隐藏 loading，显示弹窗
      chosenDir = await promptDirSelection(dirs)
      if (!chosenDir || extractionCancelled.value) {
        // 用户取消了
        appendLog('用户取消了目录选择', 'warn')
        isAddingSession.value = false
        return
      }
      isAddingSession.value = true // 恢复 loading
      appendLog(`用户选择目录: ${chosenDir}`, 'info')
    }

    if (extractionCancelled.value) { appendLog('用户已取消', 'warn'); return }

    // 第一步：提取数据库密钥
    loadingMessage.value = '正在提取数据库密钥...'
    appendLog('开始提取数据库密钥...')
    const dbRes: any = await invoke('extract_wechat_db_keys', { dataDir: chosenDir })
    if (extractionCancelled.value) { appendLog('用户已取消', 'warn'); return }
    if (!dbRes?.ok) {
      const errMsg = dbRes?.error || '提取数据库密钥失败'
      appendLog(`数据库密钥提取失败: ${errMsg}`, 'error')
      if (!extractionCancelled.value) { message.error(errMsg) }
      return
    }

    // 记录数据库密钥信息
    const dbKeys: string[] = dbRes.dbKeys || []
    appendLog(`数据库密钥提取成功，共 ${dbKeys.length} 个密钥`, 'success')
    if (dbKeys.length > 0) {
      dbKeys.forEach((k: string, i: number) => {
        // 只显示前8字符以保护隐私
        const masked = k.length > 8 ? k.slice(0, 8) + '...' : k
        appendLog(`  db_key[${i}]: ${masked}`)
      })
    }
    if (dbRes.dataDir) { appendLog(`数据目录: ${dbRes.dataDir}`) }
    if (dbRes.clientType) { appendLog(`客户端类型: ${dbRes.clientType}`) }
    if (dbRes.clientVersion) { appendLog(`客户端版本: ${dbRes.clientVersion}`) }

    // 图片密钥提取已移至 KeyInfoCard 组件中按需触发

    const res = dbRes
    if (res?.ok) {
      // 优先使用用户选择的数据目录，其次使用后端返回的
      let dataDir = chosenDir || (res.dataDir as string | null)
      if (dataDir && dataDir.startsWith('\\\\?\\')) {
        dataDir = dataDir.slice(4)
      }
      const wx_id = dataDir?.split(/[/\\]/).pop() || '';

      const clientType = res.clientType || 'win'
      const clientVersion = res.clientVersion || ''

      // 初始化新会话数据（使用新字段名，并填充旧字段以兼容现有组件）
      const draft: PartialSession = {
        name: '',
        desc: '',
        wx_id: wx_id || '',
        wx_acct_name: '',
        wx_mobile: '',
        wx_email: '',
        wx_dir: dataDir || '',
        avatar: '',
        wx_key: '',
        keys: res.dbKeys || [],
        aes_key: '',
        xor_key: '',
        client_type: clientType,
        client_version: clientVersion,
        // legacy aliases for compatibility
        wx_name: '',
        data_key: res.dataKey || ''
      }

      // 如果后端提供了本地头像路径，解析为可用的 data/url
      if (res.headImg) {
        appendLog('正在加载头像...')
        try {
          const avatarData: string = await invoke('load_avatar', { path: res.headImg })
          if (extractionCancelled.value) { appendLog('用户已取消', 'warn'); return }
          if (avatarData) {
            draft.avatar = avatarData
            appendLog('头像加载成功', 'success')
          }
        } catch (e) {
          appendLog(`头像加载失败: ${e}`, 'warn')
        }
      }

      appendLog('全部提取完成 ✓', 'success')
      if (!extractionCancelled.value) { newSessionData.value = draft }
    } else {
      const errMsg = res?.error || '提取失败，未返回可用数据'
      appendLog(`提取失败: ${errMsg}`, 'error')
      if (!extractionCancelled.value) { message.error(errMsg) }
    }
  } catch (e: any) {
    const errMsg = e?.message || String(e)
    appendLog(`调用异常: ${errMsg}`, 'error')
    if (!extractionCancelled.value) { message.error(`调用失败: ${errMsg}`) }
  } finally {
    if (!extractionCancelled.value) { isAddingSession.value = false }
  }
}

const cancelExtraction = () => {
  appendLog('用户取消了提取操作', 'warn')
  extractionCancelled.value = true
  isAddingSession.value = false
  newSessionData.value = null
  try { invoke('cancel_extract_wechat_keys') } catch {}
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

const isLoadingSessions = ref(false)
let pendingTrayRefresh = false

const loadSessions = () => {
  if (isLoadingSessions.value) { return }
  isLoadingSessions.value = true
  getSessions().then((data) => {
    const list: Session[] = Array.isArray(data) ? data as Session[] : []
    // 去重依据 id
    const uniq: Session[] = []
    const seen = new Set<number>()
    for (const s of list) { if (s && typeof s.id === 'number' && !seen.has(s.id)) { seen.add(s.id); uniq.push(s) } }
    sessions.value = uniq
    // initialize auto sync watchers for sessions marked auto_sync
    try {
      invoke('init_user_auto_sync').catch(() => {})
    } catch {}
  }).finally(() => {
    isLoadingSessions.value = false
    if (pendingTrayRefresh) { // 若托盘事件在加载过程中触发，加载结束后再刷新一次
      pendingTrayRefresh = false
      loadSessions()
    }
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

const loadSysInfo = async () => {
  try {
    const info = await getSysInfo()
    await setSysInfoToStore(info)
  } catch (e) {
    console.warn('获取系统信息失败:', e)
  }
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

.extracting-wrap { position: relative; }
</style>
