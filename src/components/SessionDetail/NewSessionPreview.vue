<template>
  <div class="content-wrapper">
    <div class="new-session-preview">
      <div class="preview-header">
        <n-icon size="32" color="#ffffff">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M12.04 2C6.58 2 2.13 6.45 2.13 11.91C2.13 13.66 2.59 15.36 3.45 16.86L2.05 22L7.3 20.62C8.75 21.41 10.38 21.83 12.04 21.83C17.5 21.83 21.95 17.38 21.95 11.92C21.95 6.45 17.5 2 12.04 2ZM12.05 20.15C10.59 20.15 9.15 19.75 7.89 19L7.55 18.83L4.42 19.65L5.25 16.61L5.06 16.26C4.24 14.96 3.81 13.47 3.81 11.91C3.81 7.37 7.49 3.69 12.04 3.69C16.58 3.69 20.26 7.37 20.26 11.91C20.26 16.45 16.58 20.15 12.05 20.15Z"/>
          </svg>
        </n-icon>
        <h2>发现新的微信账号</h2>
        <p>请确认并编辑信息，添加此微信账号到同步列表</p>
      </div>
      
      <div class="preview-content">
        <div class="preview-avatar">
          <n-avatar
            :size="80"
            :src="editable.avatar"
            :fallback-src="getDefaultAvatar(editable.wx_name)"
            round
          >
            {{ editable.wx_name ? editable.wx_name.charAt(0) : 'U' }}
          </n-avatar>
        </div>
        
        <div class="form-wrapper">
          <n-form label-placement="left" label-width="100" :model="editable" :rules="rules" ref="formRef">
            <!-- 基本信息 -->
            <div class="section">
              <div class="section-title">基本信息</div>
              <div class="grid two">
                <n-form-item label="会话名称" path="name">
                  <n-input v-model:value="editable.name" placeholder="会话名称" />
                </n-form-item>
                <n-form-item label="会话描述">
                  <n-input v-model:value="editable.desc" placeholder="备注/描述" />
                </n-form-item>
              </div>
            </div>

            <!-- 账号信息 -->
            <div class="section">
              <div class="section-title">账号信息</div>
              <div class="grid two">
                <n-form-item label="微信ID" path="wx_id">
                  <n-input v-model:value="editable.wx_id" placeholder="wxid_xxx 或账号名" />
                </n-form-item>
                <n-form-item label="昵称" path="wx_name">
                  <n-input v-model:value="editable.wx_name" placeholder="微信昵称" />
                </n-form-item>
                <n-form-item label="手机号">
                  <n-input v-model:value="editable.wx_mobile" placeholder="手机号（可选）" />
                </n-form-item>
                <n-form-item label="邮箱">
                  <n-input v-model:value="editable.wx_email" placeholder="邮箱（可选）" />
                </n-form-item>
              </div>
            </div>

            <!-- 路径与头像 -->
            <div class="section">
              <div class="section-title">数据</div>
              <div class="grid two">
                <n-form-item label="数据目录" path="wx_dir">
                  <n-input v-model:value="editable.wx_dir" placeholder="微信数据目录" />
                </n-form-item>
              </div>
            </div>

            <!-- 客户端信息 -->
            <div class="section">
              <div class="section-title">客户端信息</div>
              <div class="grid two">
                <n-form-item label="客户端类型" path="client_type">
                  <n-select
                    v-model:value="editable.client_type"
                    :options="clientTypeOptions"
                    placeholder="请选择客户端类型"
                  />
                </n-form-item>
                <n-form-item label="客户端版本" path="client_version">
                  <n-select
                    v-model:value="editable.client_version"
                    :options="clientVersionOptions"
                    placeholder="请选择客户端版本"
                  />
                </n-form-item>
              </div>
            </div>

            <!-- 密钥信息 -->
            <div class="section">
              <div class="section-title">密钥信息</div>
              <div class="grid one">
                <n-form-item label="Data Key" path="data_key">
                  <n-input v-model:value="editable.data_key" placeholder="数据密钥（hex）" />
                </n-form-item>
                <n-form-item label="AES Key" path="aes_key">
                  <n-input v-model:value="editable.aes_key" placeholder="图片密钥（hex）" />
                </n-form-item>
                <n-form-item label="XOR Key" path="xor_key">
                  <n-input v-model:value="editable.xor_key" placeholder="XOR密钥（hex）" />
                </n-form-item>
              </div>
            </div>

          </n-form>
        </div>
      </div>
      
      <div class="preview-actions">
        <n-space size="large">
          <n-button @click="handleCancel" size="large">
            取消
          </n-button>
          <n-button type="primary" @click="handleConfirm" size="large" class="wechat-btn">
            确认添加
          </n-button>
        </n-space>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, watch, ref } from 'vue'
import { NAvatar, NButton, NIcon, NSpace, NForm, NFormItem, NInput, NSelect } from 'naive-ui'
import type { FormInst, FormRules } from 'naive-ui'

interface SessionData {
  id?: number
  name: string
  desc: string
  wx_id: string
  wx_name: string
  wx_mobile: string
  wx_email: string
  wx_dir: string
  avatar: string
  online: boolean
  lastActive: string
  data_key: string
  aes_key: string
  xor_key: string
  autoSync: boolean
  syncFilters: string
  client_type: string
  client_version: string
}

interface Props {
  sessionData: Partial<SessionData>
}

interface Emits {
  (e: 'confirm', data: SessionData): void
  (e: 'cancel'): void
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const defaults: SessionData = {
  id: undefined,
  name: '',
  desc: '',
  wx_id: '',
  wx_name: '',
  wx_mobile: '',
  wx_email: '',
  wx_dir: '',
  avatar: '',
  online: true,
  lastActive: '刚刚',
  data_key: '',
  aes_key: '',
  xor_key: '',
  autoSync: false,
  syncFilters: '',
  client_type: '',
  client_version: ''
}

const editable = reactive<SessionData>({ ...defaults })

const clientTypeOptions = [
  { label: 'Windows', value: 'win' },
  { label: 'macOS', value: 'mac' }
]
const clientVersionOptions = [
  { label: 'v4', value: 'v4' },
  { label: 'v3', value: 'v3' }
]

const formRef = ref<FormInst | null>(null)
const rules: FormRules = {
  name: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('会话名称不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  wx_id: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('微信ID不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  wx_name: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('昵称不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  wx_dir: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('数据目录不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  client_type: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('客户端类型不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  client_version: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('客户端版本不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  data_key: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('Data Key 不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  aes_key: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('AES Key 不能为空'),
      trigger: ['input', 'blur']
    }
  ],
  xor_key: [
    {
      required: true,
      validator: (_rule, value: string) => !!(value && String(value).trim().length) || new Error('XOR Key 不能为空'),
      trigger: ['input', 'blur']
    }
  ]
}

watch(
  () => props.sessionData,
  (val) => {
    Object.assign(editable, { ...defaults, ...(val || {}) })
  },
  { immediate: true }
)

const getDefaultAvatar = (name: string) => {
  return `https://ui-avatars.com/api/?name=${encodeURIComponent(name || 'U')}&background=random&size=128`
}

const handleConfirm = () => {
  formRef.value?.validate((errors) => {
    if (!errors) {
      emit('confirm', { ...editable })
    }
  })
}

const handleCancel = () => {
  emit('cancel')
}
</script>

<script lang="ts">
import { defineComponent } from 'vue'

export default defineComponent({
  name: 'NewSessionPreview'
})
</script>

<style scoped>
.content-wrapper {
  padding: 20px;
  max-width: 1000px;
  margin: 0 auto;
}

.new-session-preview {
  background: white;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
  overflow: hidden;
}

.preview-header {
  text-align: center;
  padding: 32px;
  background: linear-gradient(135deg, #07c160 0%, #06ad56 100%);
  color: white;
}

.preview-header h2 {
  font-size: 24px;
  font-weight: 500;
  margin: 12px 0 8px 0;
}

.preview-header p {
  font-size: 14px;
  opacity: 0.9;
  margin: 0;
}

.preview-content {
  padding: 24px 32px 8px 32px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 24px;
}

.preview-avatar {
  text-align: center;
}

.form-wrapper {
  width: 100%;
  max-width: 800px;
}

.section {
  margin-bottom: 8px;
  padding-bottom: 8px;
  border-bottom: 1px solid #f0f0f0;
}

.section:last-child {
  border-bottom: none;
}

.section-title {
  font-size: 14px;
  font-weight: 600;
  color: #333;
  margin: 4px 0 8px 0;
}

.grid {
  display: grid;
  gap: 12px 16px;
}

.grid.one {
  grid-template-columns: 1fr;
}

.grid.two {
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
}

.preview-actions {
  padding: 24px 32px;
  background: #f8f8f8;
  text-align: center;
  border-top: 1px solid #f0f0f0;
}

.wechat-btn {
  background: #07c160 !important;
  color: white !important;
}

.wechat-btn:hover {
  background: #06ad56 !important;
}

/* 响应式设计 */
@media (max-width: 768px) {
  .content-wrapper {
    padding: 12px;
  }
  
  .preview-header {
    padding: 24px;
  }
  
  .preview-content {
    padding: 24px;
  }
  
  .preview-actions {
    padding: 20px 24px;
  }
}
</style>
