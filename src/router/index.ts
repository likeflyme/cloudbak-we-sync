import { createRouter, createWebHistory, RouteRecordRaw } from 'vue-router'
import Home from '../views/Home.vue'
import Login from '../views/Login.vue'
import SessionDetailPage from '@/views/session/SessionDetailPage.vue'
import EmptyState from '@/components/SessionDetail/EmptyState.vue'
import { isLogin } from '@/common/login'
import { getTokenFromStore } from '@/common/store'

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
      },
      {
        path: 'update',
        name: 'UpdateDetail',
        component: () => import('@/views/UpdateDetail.vue'),
        meta: { loginRequire: true }
      },
      {
        path: 'info',
        name: 'SysInfo',
        component: () => import('@/views/SysInfo.vue'),
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
  getTokenFromStore().then(token => {
    if (to.matched.some(record => record.meta.loginRequire) && !token) {
      next({ path: '/login' })
    } else {
      next()
    }
  });
})

export default router