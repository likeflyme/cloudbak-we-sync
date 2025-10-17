import { createRouter, createWebHistory, RouteRecordRaw } from 'vue-router'
import Home from '../views/Home.vue'
import Login from '../views/Login.vue'
import SessionDetailPage from '@/views/session/SessionDetailPage.vue'
import EmptyState from '@/components/SessionDetail/EmptyState.vue'
import { isLogin } from '@/common/login'

const routes: Array<RouteRecordRaw> = [
  {
    path: '/',
    name: 'Home',
    component: Home,
    meta: {
      title: 'we-sync',
      loginRequire: true
    },
    children: [
      {
        path: '',
        name: 'HomeEmpty',
        component: EmptyState,
        meta: { loginRequire: true }
      },
      {
        path: 'session/:id',
        name: 'SessionDetail',
        component: SessionDetailPage,
        meta: { loginRequire: true }
      },
      {
        path: 'settings',
        name: 'Settings',
        component: () => import('@/views/Settings.vue'),
        meta: { loginRequire: true }
      }
    ]
  },
  {
    path: '/login',
    name: 'Login',
    component: Login,
    meta: {
      title: '登录',
      loginRequire: false
    }
  }
]

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes
})

router.beforeEach((to, _from, next) => {
  // 需要登录且未登录，则重定向到 login 页
  if (to.matched.some(record => record.meta.loginRequire) && !isLogin()) {
    next({ path: '/login' })
  } else {
    next()
  }
})

export default router