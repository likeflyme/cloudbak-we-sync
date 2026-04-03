<template>
  <n-card title="密钥信息" class="info-card">
    
    <div class="key-info">
      <div class="key-item">
        <div class="key-header">
          <span class="key-label">微信数据密钥</span>
          <div class="key-actions">
            <n-button
              size="small"
              text
              @click="$emit('copyKey', keyVisibility.data_key ? (session.data_key || session.wx_key) : maskKey(session.data_key || session.wx_key))"
              class="key-action-btn"
              title="复制密钥"
            >
              <template #icon>
                <n-icon>
                  <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M19,21H8V7H19M19,5H8A2,2 0 0,0 6,7V21A2,2 0 0,0 8,23H19A2,2 0 0,0 21,21V7A2,2 0 0,0 19,5M16,1H4A2,2 0 0,0 2,3V17H4V3H16V1Z"/>
                  </svg>
                </n-icon>
              </template>
            </n-button>
            <n-button
              size="small"
              text
              @click="$emit('toggleKeyVisibility', 'data_key')"
              class="key-action-btn"
            >
              <template #icon>
                <n-icon>
                  <svg v-if="!keyVisibility.data_key" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12,9A3,3 0 0,0 9,12A3,3 0 0,0 12,15A3,3 0 0,0 15,12A3,3 0 0,0 12,9M12,17A5,5 0 0,1 7,12A5,5 0 0,1 12,7A5,5 0 0,1 17,12A5,5 0 0,1 12,17M12,4.5C7,4.5 2.73,7.61 1,12C2.73,16.39 7,19.5 12,19.5C17,19.5 21.27,16.39 23,12C21.27,7.61 17,4.5 12,4.5Z"/>
                  </svg>
                  <svg v-else viewBox="0 0 24 24" fill="currentColor">
                    <path d="M11.83,9L15,12.16C15,12.11 15,12.05 15,12A3,3 0 0,0 12,9C11.94,9 11.89,9 11.83,9M7.53,9.8L9.08,11.35C9.03,11.56 9,11.77 9,12A3,3 0 0,0 12,15C12.22,15 12.44,14.97 12.65,14.92L14.2,16.47C13.53,16.8 12.79,17 12,17A5,5 0 0,1 7,12C7,11.21 7.2,10.47 7.53,9.8M2,4.27L4.28,6.55L4.73,7C3.08,8.3 1.78,10 1,12C2.73,16.39 7,19.5 12,19.5C13.55,19.5 15.03,19.2 16.38,18.66L16.81,19.09L19.73,22L21,20.73L3.27,3M12,7A5,5 0 0,1 17,12C17,12.64 16.87,13.26 16.64,13.82L19.57,16.75C21.07,15.5 22.27,13.86 23,12C21.27,7.61 17,4.5 12,4.5C10.6,4.5 9.26,4.75 8,5.2L10.17,7.35C10.76,7.13 11.37,7 12,7Z"/>
                  </svg>
                </n-icon>
              </template>
              {{ keyVisibility.data_key ? '隐藏' : '查看' }}
            </n-button>
          </div>
        </div>
        <div class="key-value">
          {{ keyVisibility.data_key ? (session.data_key || session.wx_key) : maskKey(session.data_key || session.wx_key) }}
        </div>
      </div>
      
      <!-- 图片密钥区域 (隐藏于 v3) -->
      <template v-if="showImageKeys">
        <!-- 获取图片密钥按钮 -->
        <div class="key-item extract-img-key-item" v-if="!session.aes_key && !session.xor_key">
          <div class="key-header">
            <span class="key-label">图片密钥</span>
            <div class="key-actions">
              <n-button
                size="small"
                type="primary"
                :loading="extractingImgKeys"
                :disabled="extractingImgKeys"
                @click="handleExtractImgKeys"
                class="extract-btn"
              >
                <template #icon>
                  <n-icon>
                    <svg viewBox="0 0 24 24" fill="currentColor">
                      <path d="M17,8C8,10 5.9,16.17 3.82,21.34L5.71,22L6.66,19.7C7.14,19.87 7.64,20 8,20C19,20 22,3 22,3C21,5 14,5.25 9,6.25C4,7.25 2,11.5 2,13.5C2,15.5 3.75,17.25 3.75,17.25C7,8 17,8 17,8Z"/>
                    </svg>
                  </n-icon>
                </template>
                {{ extractingImgKeys ? '正在提取...' : '获取图片密钥' }}
              </n-button>
            </div>
          </div>
          <div class="key-hint" v-if="!extractingImgKeys && !extractImgError">
            点击按钮从微信进程内存中提取图片AES密钥和XOR密钥（需微信正在运行）
          </div>
          <div class="key-hint key-hint-error" v-if="extractImgError">
            {{ extractImgError }}
          </div>
        </div>

        <!-- 图片AES密钥 -->
        <div class="key-item" v-if="session.aes_key">
          <div class="key-header">
            <span class="key-label">图片AES密钥</span>
            <div class="key-actions">
              <n-button
                size="small"
                text
                @click="$emit('copyKey', keyVisibility.aes_key ? session.aes_key : maskKey(session.aes_key))"
                class="key-action-btn"
                title="复制密钥"
              >
                <template #icon>
                  <n-icon>
                    <svg viewBox="0 0 24 24" fill="currentColor">
                      <path d="M19,21H8V7H19M19,5H8A2,2 0 0,0 6,7V21A2,2 0 0,0 8,23H19A2,2 0 0,0 21,21V7A2,2 0 0,0 19,5M16,1H4A2,2 0 0,0 2,3V17H4V3H16V1Z"/>
                    </svg>
                  </n-icon>
                </template>
              </n-button>
              <n-button
                size="small"
                text
                @click="$emit('toggleKeyVisibility', 'aes_key')"
                class="key-action-btn"
              >
                <template #icon>
                  <n-icon>
                    <svg v-if="!keyVisibility.aes_key" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12,9A3,3 0 0,0 9,12A3,3 0 0,0 12,15A3,3 0 0,0 15,12A3,3 0 0,0 12,9M12,17A5,5 0 0,1 7,12A5,5 0 0,1 12,7A5,5 0 0,1 17,12A5,5 0 0,1 12,17M12,4.5C7,4.5 2.73,7.61 1,12C2.73,16.39 7,19.5 12,19.5C17,19.5 21.27,16.39 23,12C21.27,7.61 17,4.5 12,4.5Z"/>
                    </svg>
                    <svg v-else viewBox="0 0 24 24" fill="currentColor">
                      <path d="M11.83,9L15,12.16C15,12.11 15,12.05 15,12A3,3 0 0,0 12,9C11.94,9 11.89,9 11.83,9M7.53,9.8L9.08,11.35C9.03,11.56 9,11.77 9,12A3,3 0 0,0 12,15C12.22,15 12.44,14.97 12.65,14.92L14.2,16.47C13.53,16.8 12.79,17 12,17A5,5 0 0,1 7,12C7,11.21 7.2,10.47 7.53,9.8M2,4.27L4.28,6.55L4.73,7C3.08,8.3 1.78,10 1,12C2.73,16.39 7,19.5 12,19.5C13.55,19.5 15.03,19.2 16.38,18.66L16.81,19.09L19.73,22L21,20.73L3.27,3M12,7A5,5 0 0,1 17,12C17,12.64 16.87,13.26 16.64,13.82L19.57,16.75C21.07,15.5 22.27,13.86 23,12C21.27,7.61 17,4.5 12,4.5C10.6,4.5 9.26,4.75 8,5.2L10.17,7.35C10.76,7.13 11.37,7 12,7Z"/>
                    </svg>
                  </n-icon>
                </template>
                {{ keyVisibility.aes_key ? '隐藏' : '查看' }}
              </n-button>
            </div>
          </div>
          <div class="key-value">
            {{ keyVisibility.aes_key ? session.aes_key : maskKey(session.aes_key) }}
          </div>
        </div>
        
        <!-- 图片XOR密钥 -->
        <div class="key-item" v-if="session.xor_key">
          <div class="key-header">
            <span class="key-label">图片XOR密钥</span>
            <div class="key-actions">
              <n-button
                size="small"
                text
                @click="$emit('copyKey', keyVisibility.xor_key ? session.xor_key : maskKey(session.xor_key))"
                class="key-action-btn"
                title="复制密钥"
              >
                <template #icon>
                  <n-icon>
                    <svg viewBox="0 0 24 24" fill="currentColor">
                      <path d="M19,21H8V7H19M19,5H8A2,2 0 0,0 6,7V21A2,2 0 0,0 8,23H19A2,2 0 0,0 21,21V7A2,2 0 0,0 19,5M16,1H4A2,2 0 0,0 2,3V17H4V3H16V1Z"/>
                    </svg>
                  </n-icon>
                </template>
              </n-button>
            </div>
          </div>
          <div class="key-value">
            {{ session.xor_key }}
          </div>
        </div>
        <!-- 重新获取图片密钥按钮 -->
        <div class="key-item extract-img-key-item" v-if="session.aes_key || session.xor_key">
          <n-button
            size="small"
            type="success"
            @click="handleReExtractImgKeys"
            :loading="extractingImgKeys"
            :disabled="extractingImgKeys"
          >
            <template #icon>
              <n-icon>
                <svg viewBox="0 0 24 24" fill="currentColor">
                  <path d="M17.65,6.35C16.2,4.9 14.21,4 12,4A8,8 0 0,0 4,12A8,8 0 0,0 12,20C15.73,20 18.84,17.45 19.73,14H17.65C16.83,16.33 14.61,18 12,18A6,6 0 0,1 6,12A6,6 0 0,1 12,6C13.66,6 15.14,6.69 16.22,7.78L13,11H20V4L17.65,6.35Z"/>
                </svg>
              </n-icon>
            </template>
            {{ extractingImgKeys ? '正在提取...' : '获取图片密钥' }}
          </n-button>
        </div>
      </template>
    </div>

    <!-- 提取图片密钥的模态弹窗 -->
    <n-modal
      v-model:show="showExtractModal"
      preset="card"
      title="提取图片密钥"
      style="max-width: 460px;"
      :mask-closable="false"
      :closable="!extractingImgKeys"
      :close-on-esc="!extractingImgKeys"
    >
      <div class="extract-modal-body">
        <template v-if="extractingImgKeys">
          <div class="extract-spinner">
            <n-spin size="medium" />
          </div>
          <div class="extract-modal-text">正在从微信进程内存中提取图片密钥，请稍候...</div>
          <div class="extract-modal-hint">此过程需要扫描进程内存，可能需要数十秒</div>
        </template>
        <template v-else-if="extractImgError">
          <div class="extract-modal-text extract-modal-error">{{ extractImgError }}</div>
        </template>
        <template v-else-if="extractImgSuccess">
          <div class="extract-modal-text extract-modal-success">图片密钥提取成功 ✓</div>
        </template>
      </div>
      <template #footer>
        <n-space justify="end">
          <n-button
            v-if="!extractingImgKeys"
            @click="showExtractModal = false"
          >
            关闭
          </n-button>
        </n-space>
      </template>
    </n-modal>
  </n-card>
</template>

<script setup lang="ts">
import { NCard, NIcon, NButton, NModal, NSpin, NSpace, createDiscreteApi } from 'naive-ui';
import { invoke } from '@tauri-apps/api/core';
import type { Session } from '@/models/session';
import { computed, ref } from 'vue';

const { dialog } = createDiscreteApi(['dialog'])

// 单次定义 props，避免重复调用 defineProps
const props = defineProps<{
  session: Session;
  keyVisibility: {
    data_key: boolean;
    aes_key: boolean;
    xor_key: boolean;
  };
}>();

const emit = defineEmits<{
  (e: 'copyKey', key: string): void;
  (e: 'toggleKeyVisibility', key: 'data_key' | 'aes_key' | 'xor_key'): void;
  (e: 'updateImgKeys', payload: { aes_key: string; xor_key: string }): void;
}>();

// 根据客户端版本决定是否显示图片相关密钥 (v3 不显示)
// 兼容 "v3", "3", "3.0.0", "v3.1" 等格式
const showImageKeys = computed(() => {
  const raw = props.session.client_version || ''
  const cleaned = raw.trim().toLowerCase().replace(/^v/, '')
  const major = cleaned.split(/[._-]/)[0]
  return major !== '3'
});

// 脱敏显示密钥
const maskKey = (key: string) => {
  if (!key || key.length < 8) return key
  return key.substring(0, 4) + '****' + key.substring(key.length - 4)
}

// --- 图片密钥提取逻辑 ---
const extractingImgKeys = ref(false)
const extractImgError = ref('')
const extractImgSuccess = ref(false)
const showExtractModal = ref(false)

/** ASCII 文本转十六进制字符串，例如 "abc" -> "616263" */
const asciiToHex = (str: string): string => {
  return Array.from(str)
    .map(c => c.charCodeAt(0).toString(16).padStart(2, '0'))
    .join('')
}

/** 将后端返回的 xor_key（可能是 "0x1a" 或 "26" 等格式）转换为十进制字符串 */
const xorKeyToDecimal = (raw: string): string => {
  if (!raw) return ''
  const trimmed = raw.trim().toLowerCase()
  let num: number
  if (trimmed.startsWith('0x')) {
    num = parseInt(trimmed, 16)
  } else {
    num = parseInt(trimmed, 10)
  }
  return isNaN(num) ? raw : String(num)
}

const handleReExtractImgKeys = () => {
  dialog.warning({
    title: '获取图片密钥',
    content: '获取将覆盖当前已有的图片AES密钥和XOR密钥，确认继续？',
    positiveText: '确认',
    negativeText: '取消',
    onPositiveClick: () => {
      handleExtractImgKeys()
    }
  })
}

const handleExtractImgKeys = async () => {
  // 防止重复触发
  if (extractingImgKeys.value) return

  // 打开弹窗，重置状态
  extractingImgKeys.value = true
  extractImgError.value = ''
  extractImgSuccess.value = false
  showExtractModal.value = true

  try {
    const dataDir = props.session.wx_dir || null
    const imgRes: any = await invoke('extract_wechat_img_keys', { dataDir })
    if (imgRes?.ok) {
      // aes_key: 后端返回的是 ASCII 文本，转成十六进制
      const rawAesKey: string = imgRes.imageKey || ''
      const aesKeyHex = rawAesKey ? asciiToHex(rawAesKey) : ''

      // xor_key: 后端返回的可能是 "0x1a" 格式，转成十进制
      const rawXorKey: string = imgRes.xorKey != null ? String(imgRes.xorKey) : ''
      const xorKeyDecimal = rawXorKey ? xorKeyToDecimal(rawXorKey) : ''

      if (!aesKeyHex || !xorKeyDecimal) {
        const missing = !aesKeyHex && !xorKeyDecimal
          ? 'AES Key 和 XOR Key 均为空'
          : !aesKeyHex ? 'AES Key 为空' : 'XOR Key 为空'
        extractImgError.value = `提取失败：${missing}，查看微信中多张图片后快速重新获取`
        return
      }

      extractImgSuccess.value = true
      // 通知父组件更新 session 的 aes_key 和 xor_key
      emit('updateImgKeys', { aes_key: aesKeyHex, xor_key: xorKeyDecimal })

      // 成功后自动关闭弹窗
      setTimeout(() => { showExtractModal.value = false }, 800)
    } else {
      extractImgError.value = imgRes?.error || '提取图片密钥失败'
    }
  } catch (e: any) {
    extractImgError.value = `提取异常: ${e?.message || String(e)}`
  } finally {
    extractingImgKeys.value = false
  }
}
</script>

<style scoped>
.info-card {
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
  overflow: hidden;
  transition: all 0.2s ease;
  background: white;
}

.info-card:hover {
  transform: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

/* 密钥信息样式 */
.key-info {
  padding: 8px 0;
}

.key-item {
  margin-bottom: 20px;
  padding: 16px;
  background: #f8f8f8;
  border-radius: 6px;
  border-left: 3px solid #07c160;
}

.key-item:last-child {
  margin-bottom: 0;
}

.key-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.key-label {
  font-size: 14px;
  font-weight: 500;
  color: #666666;
}

.key-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.key-action-btn {
  color: #666666 !important;
  font-size: 12px !important;
  padding: 4px 8px !important;
  background: transparent !important;
  border: 1px solid #d9d9d9 !important;
  border-radius: 4px !important;
}

.key-action-btn:hover {
  color: #333333 !important;
  background: #f5f5f5 !important;
  border-color: #bfbfbf !important;
}

.key-action-btn:focus {
  color: #333333 !important;
  background: #f5f5f5 !important;
  border-color: #bfbfbf !important;
  box-shadow: 0 0 0 2px rgba(102, 102, 102, 0.1) !important;
}

.key-value {
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Fira Code', 'Fira Mono', 'Roboto Mono', monospace;
  font-size: 13px;
  color: #333333;
  background: #ffffff;
  border: 1px solid #e7e7e7;
  border-radius: 4px;
  padding: 10px 12px;
  word-break: break-all;
  line-height: 1.4;
}

.extract-img-key-item {
  border-left-color: #e6a700;
}

.extract-btn {
  font-size: 12px !important;
}

.key-hint {
  font-size: 12px;
  color: #999;
  line-height: 1.5;
}

.key-hint-error {
  color: #d03050;
}

/* 提取弹窗样式 */
.extract-modal-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 20px 0;
  min-height: 100px;
  justify-content: center;
}

.extract-spinner {
  margin-bottom: 16px;
}

.extract-modal-text {
  font-size: 15px;
  color: #333;
  text-align: center;
  line-height: 1.6;
}

.extract-modal-hint {
  font-size: 13px;
  color: #999;
  margin-top: 8px;
  text-align: center;
}

.extract-modal-error {
  color: #d03050;
  font-weight: 500;
}

.extract-modal-success {
  color: #18a058;
  font-weight: 500;
  font-size: 16px;
}

/* 卡片标题样式 */
.n-card .n-card-header {
  padding: 16px 20px;
  border-bottom: 1px solid #f0f0f0;
  font-weight: 500;
  color: #333333;
}

.n-card .n-card-header .n-card-header__extra {
  display: flex;
  align-items: center;
}

.n-card {
  border-radius: 8px;
}
</style>
