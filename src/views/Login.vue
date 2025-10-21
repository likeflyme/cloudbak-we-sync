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

      <n-form ref="formRef" :model="form" :rules="rules" size="large">
        <n-form-item label="服务器地址" path="endpoint">
          <n-input
            v-model:value="form.endpoint"
            placeholder="例如：https://192.168.1.10:443"
            clearable
          />
        </n-form-item>

        <n-form-item label="账号" path="username">
          <n-input
            v-model:value="form.username"
            placeholder="用户名或邮箱"
            clearable
            autofocus
          />
        </n-form-item>

        <n-form-item label="密码" path="password">
          <n-input
            v-model:value="form.password"
            type="password"
            show-password-on="mousedown"
            placeholder="填写密码"
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
import { invoke } from '@tauri-apps/api/core'
import { saveToken } from '@/common/login'

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
onMounted(() => {
  const savedEndpoint = localStorage.getItem('endpoint')
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
  error_msg.value = ''
  try {
    await formRef.value?.validate()
  } catch {
    error_msg.value = '请完整填写信息后重试'
    return
  }

  is_loading.value = true
  login_btn_title.value = '登录中…'
  try {

    const payload = {
      endpoint: form.endpoint.trim(),
      username: form.username,
      password: form.password
    }

    userLogin(payload).then(resp => {
      if (resp.status === 200) {
        resp.json().then(d => {
          let token = d.token_type + " " + d.access_token;
          saveToken(token);
          localStorage.setItem('endpoint', payload.endpoint); // 保留 endpoint 也可改 store
          // fetch current user info
          fetchMe(payload.endpoint, token).then(r => {
            if (r.status === 200) {
              r.json().then(info => {
                if (info && (info.id !== undefined)) {
                  try { invoke('persist_auth', { userId: Number(info.id), token, baseUrl: payload.endpoint + '/api' }) } catch {}
                }
                router.push('/');
              });
            } else {
              router.push('/');
            }
          }).catch(() => router.push('/'));
        });
      } else {
        resp.json().then(d => {
          error_msg.value = d.detail || '登录失败，请稍后重试';
          closeLogin();
        });
      }
    });
  } catch (e: any) {
    error_msg.value = e?.message || '登录失败，请稍后重试'
    closeLogin();
  }
}

const closeLogin = () => {
  is_loading.value = false;
  login_btn_title.value = '登 录'
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
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.06);
  border-radius: 12px;
}

.actions {
  margin-top: 8px;
}
.actions :deep(.n-button) {
  width: 100%;
}

.mb-16 { margin-bottom: 16px; }

.footer {
  margin-top: 16px;
  color: #909399;
  font-size: 12px;
}

.footer .link {
  color: #07C160;
  text-decoration: none;
}

.footer .sep {
  margin: 0 8px;
  color: #c0c4cc;
}

/* 防止触控设备的弹性滚动产生视觉滚动条 */
:host { overscroll-behavior: none; }
</style>