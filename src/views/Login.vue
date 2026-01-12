<template>
  <div class="login-page">
    <div class="brand">
      <div class="logo-circle">云</div>
      <div class="brand-text">
        <div class="title">云朵备份</div>
        <div class="subtitle">登录您的微信备份助手</div>
      </div>
    </div>

    <n-card class="login-card" :bordered="false">
      <n-alert v-if="error_msg" type="warning" show-icon closable @close="error_msg = ''" class="mb-16">
        {{ error_msg }}
      </n-alert>

      <n-form ref="formRef" :model="form" :rules="rules" size="large" @keydown.enter.prevent="login">
        <n-form-item label="服务器地址" path="endpoint">
          <n-input
            v-model:value="form.endpoint"
            placeholder="例如：https://192.168.1.10:443"
            clearable
            @keyup.enter.prevent="login"
          />
        </n-form-item>

        <n-form-item label="账号" path="username">
          <n-input
            v-model:value="form.username"
            placeholder="用户名或邮箱"
            clearable
            autofocus
            @keyup.enter.prevent="login"
          />
        </n-form-item>

        <n-form-item label="密码" path="password">
          <n-input
            v-model:value="form.password"
            type="password"
            show-password-on="mousedown"
            placeholder="填写密码"
            @keyup.enter.prevent="login"
          />
        </n-form-item>

        <div class="actions">
          <n-button
            type="primary"
            :color="wechatGreen"
            :loading="is_loading"
            size="large"
            round
            @click="login"
          >
            {{ login_btn_title }}
          </n-button>
        </div>
      </n-form>
    </n-card>

    <div class="footer">
      <a href="javascript:" class="link">云朵备份</a>
      <span class="sep">·</span>
      <span class="copy">Copyright © 2024-2025 cloudbak.org</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import type { FormInst, FormRules } from 'naive-ui'
import { NCard, NForm, NFormItem, NInput, NButton, NAlert } from 'naive-ui'
import { login as userLogin, me as fetchMe } from '@/api/auth'
import { setTokenToStore, setEndpointToStore, setUserInfoToStore, getEndpointFromStore } from '@/common/store'
// import { invoke } from '@tauri-apps/api/core'

const router = useRouter()

const error_msg = ref('')
const is_loading = ref(false)
const login_btn_title = ref('登 录')
const wechatGreen = '#07C160'

// 登录表单模型
type LoginForm = {
  endpoint: string
  username: string
  password: string
}

const form = reactive<LoginForm>({
  endpoint: '',
  username: '',
  password: ''
})

// Prefill endpoint from localStorage when visiting the page
onMounted(async () => {
  const savedEndpoint = await getEndpointFromStore();
  if (savedEndpoint) {
    form.endpoint = savedEndpoint
  }
})

const formRef = ref<FormInst | null>(null)

const rules = computed<FormRules>(() => ({
  endpoint: [
    { required: true, message: '请输入服务器地址', trigger: ['input', 'blur'] },
    {
      validator: (_: any, value: string) => {
        try {
          const u = new URL(value.trim())
          if (!['http:', 'https:'].includes(u.protocol)) return false
          if (!u.hostname) return false
          if (u.port) {
            const p = Number(u.port)
            if (!Number.isInteger(p) || p < 1 || p > 65535) return false
          }
          return true
        } catch {
          return false
        }
      },
      message: '请输入有效地址（例如：https://192.168.1.10:443）',
      trigger: ['blur', 'input']
    }
  ],
  username: [{ required: true, message: '请输入账号', trigger: ['input', 'blur'] }],
  password: [{ required: true, message: '请输入密码', trigger: ['input', 'blur'] }]
}))

const login = async () => {
  // const resp = await invoke('auth_test_login', {
  //   endpoint: form.endpoint.trim(),
  //   username: form.username,
  //   password: form.password
  // })
  // console.log('auth_test_login response:', resp)

  error_msg.value = ''
  try {
    await formRef.value?.validate()
  } catch {
    error_msg.value = '请完整填写信息后重试'
    return
  }

  is_loading.value = true
  login_btn_title.value = '登录中…'

  const payload = {
    endpoint: form.endpoint.trim(),
    username: form.username,
    password: form.password
  }

  try {
    const resp = await userLogin(payload)

    const contentType = resp.headers.get('content-type') || ''
    const isJson = contentType.toLowerCase().includes('application/json')

    const readTextSafely = async () => {
      try {
        return await resp.text()
      } catch {
        return ''
      }
    }

    // Some backends may return empty body; avoid resp.json() throwing.
    const safeJson = async () => {
      if (!isJson) return null
      const text = await readTextSafely()
      if (!text) return null
      try {
        return JSON.parse(text)
      } catch {
        return null
      }
    }

    if (resp.status === 200) {
      const d: any = await safeJson()
      if (!d?.access_token || !d?.token_type) {
        throw new Error('登录接口返回格式异常（缺少 token）')
      }

      const token = `${d.token_type} ${d.access_token}`
      await Promise.allSettled([setTokenToStore(token), setEndpointToStore(payload.endpoint)])

      try {
        const meResp = await fetchMe(payload.endpoint, token)
        if (meResp.status === 200) {
          const meCt = meResp.headers.get('content-type') || ''
          const meText = await meResp.text()
          const info =
            meCt.toLowerCase().includes('application/json') && meText ? JSON.parse(meText) : null
          if (info && info.id !== undefined) {
            await Promise.allSettled([setUserInfoToStore(info)])
          }
        }
      } catch {
        // ignore, still go home
      }

      router.push('/')
      return
    }

    // Non-200: show server message if possible
    const errBody: any = await safeJson()
    const detail = errBody?.detail || errBody?.message || errBody?.error

    if (resp.status === 401) {
      error_msg.value = detail || '账号或密码错误'
    } else {
      error_msg.value = detail ? `登录失败（${resp.status}）：${detail}` : `登录失败（${resp.status}）`
    }
  } catch (e: any) {
    // Network error / CORS / DNS / refused / timeout etc.
    const msg = (e?.message || '').toString()
    if (msg) {
      error_msg.value = `网络异常：${msg}`
    } else {
      error_msg.value = '网络异常：请求失败'
    }
  } finally {
    is_loading.value = false
    login_btn_title.value = '登 录'
  }
}
</script>

<style scoped lang="less">
.login-page {
  position: fixed;
  inset: 0; // top:0; right:0; bottom:0; left:0
  overflow: hidden; // 禁止页面滚动条
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: #f7f7f7;
  padding: 24px 16px;
}

.brand {
  display: flex;
  align-items: center;
  margin-bottom: 16px;
}

.logo-circle {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: #07C160;
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  margin-right: 12px;
}

.brand-text .title {
  font-size: 20px;
  font-weight: 600;
}

.brand-text .subtitle {
  font-size: 12px;
  color: #8c8c8c;
}

.login-card {
  width: 100%;
  max-width: 420px;
  border-radius: 12px;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.08);
}

.actions {
  margin-top: 8px;
  width: 100%;

  :deep(.n-button) {
    width: 100%;
  }
}

.footer {
  margin-top: 18px;
  font-size: 12px;
  color: #999;
}

.footer .link {
  color: #07C160;
  text-decoration: none;
}

.footer .sep {
  margin: 0 8px;
}

.mb-16 {
  margin-bottom: 16px;
}
</style>