<template>
  <n-card title="密钥信息" class="info-card">
    <template #header-extra>
      <n-icon size="20" color="#07c160">
        <svg viewBox="0 0 24 24" fill="currentColor">
          <path d="M7,14A3,3 0 0,0 10,17A3,3 0 0,0 13,14A3,3 0 0,0 10,11A3,3 0 0,0 7,14M12.65,10A1,1 0 0,1 13.65,11L20.31,4.27L21.72,5.68L15.04,12.36C15.65,13.05 16,13.97 16,15A5,5 0 0,1 11,20A5,5 0 0,1 6,15A5,5 0 0,1 11,10C12.03,10 12.95,10.35 13.65,11L14.46,10.18L4.27,21.72L2.86,20.31L12.54,10.63C12.58,10.57 12.62,10.5 12.65,10Z"/>
        </svg>
      </n-icon>
    </template>
    <div class="key-info">
      <div class="key-item">
        <div class="key-header">
          <span class="key-label">微信数据密钥</span>
          <div class="key-actions">
            <n-button
              size="small"
              text
              @click="$emit('copyKey', keyVisibility.data_key ? session.data_key : maskKey(session.data_key))"
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
          {{ keyVisibility.data_key ? session.data_key : maskKey(session.data_key) }}
        </div>
      </div>
      
      <div class="key-item">
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
      
      <div class="key-item">
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
            <n-button
              size="small"
              text
              @click="$emit('toggleKeyVisibility', 'xor_key')"
              class="key-action-btn"
            >
              <template #icon>
                <n-icon>
                  <svg v-if="!keyVisibility.xor_key" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12,9A3,3 0 0,0 9,12A3,3 0 0,0 12,15A3,3 0 0,0 15,12A3,3 0 0,0 12,9M12,17A5,5 0 0,1 7,12A5,5 0 0,1 12,7A5,5 0 0,1 17,12A5,5 0 0,1 12,17M12,4.5C7,4.5 2.73,7.61 1,12C2.73,16.39 7,19.5 12,19.5C17,19.5 21.27,16.39 23,12C21.27,7.61 17,4.5 12,4.5Z"/>
                  </svg>
                  <svg v-else viewBox="0 0 24 24" fill="currentColor">
                    <path d="M11.83,9L15,12.16C15,12.11 15,12.05 15,12A3,3 0 0,0 12,9C11.94,9 11.89,9 11.83,9M7.53,9.8L9.08,11.35C9.03,11.56 9,11.77 9,12A3,3 0 0,0 12,15C12.22,15 12.44,14.97 12.65,14.92L14.2,16.47C13.53,16.8 12.79,17 12,17A5,5 0 0,1 7,12C7,11.21 7.2,10.47 7.53,9.8M2,4.27L4.28,6.55L4.73,7C3.08,8.3 1.78,10 1,12C2.73,16.39 7,19.5 12,19.5C13.55,19.5 15.03,19.2 16.38,18.66L16.81,19.09L19.73,22L21,20.73L3.27,3M12,7A5,5 0 0,1 17,12C17,12.64 16.87,13.26 16.64,13.82L19.57,16.75C21.07,15.5 22.27,13.86 23,12C21.27,7.61 17,4.5 12,4.5C10.6,4.5 9.26,4.75 8,5.2L10.17,7.35C10.76,7.13 11.37,7 12,7Z"/>
                  </svg>
                </n-icon>
              </template>
              {{ keyVisibility.xor_key ? '隐藏' : '查看' }}
            </n-button>
          </div>
        </div>
        <div class="key-value">
          {{ keyVisibility.xor_key ? session.xor_key : maskKey(session.xor_key) }}
        </div>
      </div>
    </div>
  </n-card>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { NCard, NIcon, NButton } from 'naive-ui';
import type { SessionData } from '../Session/SessionSidebar.vue';

defineProps<{
  session: SessionData;
}>();

// 密钥显示状态
const keyVisibility = ref({
  data_key: false,
  aes_key: false,
  xor_key: false
});

// 脱敏显示密钥
const maskKey = (key: string) => {
  if (!key || key.length < 8) return key
  return key.substring(0, 4) + '****' + key.substring(key.length - 4)
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
