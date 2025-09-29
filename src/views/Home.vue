<template>
  <n-layout has-sider class="h-screen">
    <!-- 左侧会话列表 -->
    <SessionSidebar 
      :sessions="sessions"
      :selected-id="selected?.id"
      @select-session="selectSession"
      @add-session="showAddDialog"
    />

    <!-- 右侧详情区域 -->
    <n-layout-content class="main-content">
      <!-- 添加会话状态 -->
      <LoadingState v-if="isAddingSession" />
      
      <!-- 新会话信息确认 -->
      <NewSessionPreview 
        v-else-if="newSessionData" 
        :session-data="newSessionData"
        @confirm="confirmAdd"
        @cancel="cancelAdd"
      />
      
      <!-- 原有的会话详情显示 -->
      <div v-else-if="selected" class="content-wrapper">
        <!-- 用户信息头部 -->
        <UserHeader 
          :session="selected" 
          @toggle-auto-sync="toggleAutoSync"
        />

        <!-- 详细信息卡片 -->
        <div class="info-cards">
          <BasicInfoCard :session="selected" />
          <StorageInfoCard :session="selected" />
          <KeyInfoCard 
            :session="selected" 
            :key-visibility="keyVisibility"
            @toggle-key-visibility="toggleKeyVisibility"
            @copy-key="copyKey"
          />
        </div>

        <!-- 同步过滤配置 -->
        <FilterConfigCard 
          :sync-filters="selected.syncFilters"
          @update:syncFilters="updateSyncFilters"
        />

        <!-- 操作按钮区域 -->
        <ActionButtons 
          @sync="handleSync"
          @delete="deleteSession"
        />
      </div>

      <!-- 没选中会话时 -->
      <EmptyState v-else />
    </n-layout-content>
  </n-layout>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { NLayout, NLayoutContent } from 'naive-ui';
import { invoke } from '@tauri-apps/api/core'

// 导入组件
import SessionSidebar from '@/components/Session/SessionSidebar.vue'
import UserHeader from '@/components/SessionDetail/UserHeader.vue'
import BasicInfoCard from '@/components/SessionDetail/BasicInfoCard.vue'
import StorageInfoCard from '@/components/SessionDetail/StorageInfoCard.vue'
import KeyInfoCard from '@/components/SessionDetail/KeyInfoCard.vue'
import FilterConfigCard from '@/components/SessionDetail/FilterConfigCard.vue'
import ActionButtons from '@/components/SessionDetail/ActionButtons.vue'
import NewSessionPreview from '@/components/SessionDetail/NewSessionPreview.vue'
import LoadingState from '@/components/SessionDetail/LoadingState.vue'
import EmptyState from '@/components/SessionDetail/EmptyState.vue'

// 模拟数据，实际应从 API 获取
const sessions = ref([
  { 
    id: 1, 
    name: '会话A', 
    desc: '描述1', 
    wx_id: 'wxid_a', 
    wx_name: '小明', 
    wx_mobile: '13800000000', 
    wx_email: 'a@example.com', 
    wx_dir: '/chat/a',
    avatar: 'https://avatars.githubusercontent.com/u/1?v=4',
    online: true,
    lastActive: '2分钟前',
    data_key: '1234567890abcdef1234567890abcdef12345678',
    aes_key: 'abcdef1234567890abcdef1234567890abcdef12',
    xor_key: '9876543210fedcba9876543210fedcba98765432',
    autoSync: true,
    syncFilters: "*.log\n*.tmp\n.DS_Store\nnode_modules/\n.git/\ntemp/"
  },
  { 
    id: 2, 
    name: '会话B', 
    desc: '描述2', 
    wx_id: 'wxid_b', 
    wx_name: '小红', 
    wx_mobile: '13900000000', 
    wx_email: 'b@example.com', 
    wx_dir: '/chat/b',
    avatar: 'https://avatars.githubusercontent.com/u/2?v=4',
    online: false,
    lastActive: '1小时前',
    data_key: 'fedcba0987654321fedcba0987654321fedcba09',
    aes_key: '0987654321fedcba0987654321fedcba09876543',
    xor_key: '1357924680acedfb1357924680acedfb13579246',
    autoSync: false,
    syncFilters: "*.bak\n*.cache\nbackup/\ncache/"
  }
])

const selected = ref<typeof sessions.value[0] | null>(null)
const isAddingSession = ref(false)
const newSessionData = ref<any>(null)

// 密钥显示状态
const keyVisibility = ref({
  data_key: false,
  aes_key: false,
  xor_key: false
})

const selectSession = (s: any) => {
  selected.value = s
  newSessionData.value = null
  // 重置密钥可见性状态
  keyVisibility.value = {
    data_key: false,
    aes_key: false,
    xor_key: false
  }
}

const deleteSession = () => {
  if (selected.value) {
    console.log('删除会话:', selected.value.id)
    // 从sessions中删除
    const index = sessions.value.findIndex(s => s.id === selected.value?.id)
    if (index !== -1) {
      sessions.value.splice(index, 1)
      selected.value = null
    }
  }
}

const handleSync = (key: string) => {
  if (selected.value) {
    console.log('对会话', selected.value.id, '执行操作:', key)
  }
}

// 切换密钥可见性
const toggleKeyVisibility = (keyType: 'data_key' | 'aes_key' | 'xor_key') => {
  keyVisibility.value[keyType] = !keyVisibility.value[keyType]
}

// 切换自动同步
const toggleAutoSync = () => {
  if (selected.value) {
    selected.value.autoSync = !selected.value.autoSync
    console.log('自动同步状态已切换为:', selected.value.autoSync ? '开启' : '关闭')
  }
}

// 更新同步过滤配置
const updateSyncFilters = (value: string) => {
  if (selected.value) {
    selected.value.syncFilters = value
  }
}

// 拷贝密钥到剪贴板
const copyKey = async (key: string) => {
  try {
    await navigator.clipboard.writeText(key)
    console.log('密钥已复制到剪贴板')
  } catch (err) {
    console.error('复制失败:', err)
    // 降级处理：使用旧的方式复制
    try {
      const textArea = document.createElement('textarea')
      textArea.value = key
      document.body.appendChild(textArea)
      textArea.focus()
      textArea.select()
      document.execCommand('copy')
      document.body.removeChild(textArea)
      console.log('密钥已复制到剪贴板（降级方式）')
    } catch (fallbackError) {
      console.error('复制失败（降级方式）:', fallbackError)
    }
  }
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

      let dataDir = res.dataDir
      if (dataDir && dataDir.startsWith('\\\\?\\')) {
        dataDir = dataDir.slice(4)
      }

      const clientType = res.clientType || 'win'
      const clientVersion = res.clientVersion || ''

      newSessionData.value = {
        name: '',
        desc: '',
        wx_id: account || '',
        wx_name: '',
        wx_mobile: '',
        wx_email: '',
        wx_dir: dataDir || '',
        avatar: '',
        data_key: res.dataKey || '',
        aes_key: res.imageKey || '',
        xor_key: res.xorKey,
        client_type: clientType,
        client_version: clientVersion
      }
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
const confirmAdd = (sessionData: any) => {
  if (sessionData) {
    const newSession = {
      id: sessions.value.length + 1,
      // 保留用户在预览页修改后的所有字段
      ...sessionData,
      // 一些兜底字段
      online: sessionData.online ?? true,
      lastActive: sessionData.lastActive ?? '刚刚',
      autoSync: sessionData.autoSync ?? false,
      syncFilters: sessionData.syncFilters ?? ''
    }

    sessions.value.push(newSession)
    selected.value = newSession
    newSessionData.value = null
  }
}
</script>

<style scoped>
.h-screen {
  height: 100vh;
  background: #f7f7f7;
}

.main-content {
  background: #f7f7f7;
}

.content-wrapper {
  padding: 20px;
  max-width: 1000px;
  margin: 0 auto;
}

/* 信息卡片样式 */
.info-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
  gap: 16px;
  margin-bottom: 20px;
}

/* 响应式设计 */
@media (max-width: 1200px) {
  .content-wrapper {
    padding: 16px;
  }
  
  .info-cards {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 768px) {
  .content-wrapper {
    padding: 12px;
  }
}

/* 动画效果 */
@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.content-wrapper > * {
  animation: fadeIn 0.3s ease-out;
}
</style>
