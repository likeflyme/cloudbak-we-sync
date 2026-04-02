<template>
  <div class="content-wrapper">
    <div class="loading-state">
      <div class="loading-text">{{ message }}</div>
      <!-- 取消按钮 -->
      <div v-if="canCancel" class="cancel-inline">
        <a href="javascript:" class="cancel-link" @click="$emit('cancel')">取消</a>
      </div>
      <!-- 日志区域 -->
      <div v-if="logs.length > 0" class="log-area" ref="logAreaRef">
        <div v-for="(log, idx) in logs" :key="idx" :class="['log-line', log.level]">
          <span class="log-time">{{ log.time }}</span>
          <span class="log-msg">{{ log.text }}</span>
        </div>
      </div>
      
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick, watch } from 'vue'

export interface LogEntry {
  time: string
  text: string
  level: 'info' | 'warn' | 'error' | 'success'
}

interface Props {
  message?: string
  logs?: LogEntry[]
  canCancel?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  message: '正在扫描微信数据...',
  logs: () => [],
  canCancel: false,
})

defineEmits<{
  (e: 'cancel'): void
}>()

const logAreaRef = ref<HTMLElement | null>(null)

// Auto-scroll to bottom when new logs arrive
watch(() => props.logs.length, async () => {
  await nextTick()
  if (logAreaRef.value) {
    logAreaRef.value.scrollTop = logAreaRef.value.scrollHeight
  }
})
</script>

<style scoped>
.content-wrapper {
  padding: 20px;
  max-width: 1000px;
  margin: 0 auto;
}

.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 400px;
  background: white;
  border-radius: 8px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  border: 1px solid #e7e7e7;
  padding: 24px;
}

.loading-text {
  margin-top: 16px;
  font-size: 16px;
  color: #666666;
}

.log-area {
  width: 100%;
  max-height: 400px;
  overflow-y: auto;
  margin-top: 20px;
  padding: 10px 12px;
  background: #f9f9f9;
  border: 1px solid #eaeaea;
  border-radius: 6px;
  font-family: 'Menlo', 'Consolas', 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.7;
}

.log-line {
  display: flex;
  gap: 8px;
  white-space: nowrap;
}

.log-time {
  color: #aaa;
  flex-shrink: 0;
}

.log-msg {
  white-space: pre-wrap;
  word-break: break-all;
}

.log-line.info .log-msg { color: #555; }
.log-line.warn .log-msg { color: #e6a700; }
.log-line.error .log-msg { color: #d03050; }
.log-line.success .log-msg { color: #18a058; }

.cancel-inline { text-align: center; margin-top: 12px; }
.cancel-link { display: inline-block; font-size: 13px; color: #409eff; text-decoration: underline; cursor: pointer; }
.cancel-link:hover { color: #66b1ff; }

/* 响应式设计 */
@media (max-width: 768px) {
  .content-wrapper {
    padding: 12px;
  }

  .loading-state {
    min-height: 300px;
  }

  .log-area {
    max-height: 150px;
  }
}
</style>
