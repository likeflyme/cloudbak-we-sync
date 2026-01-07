import { Store } from '@tauri-apps/plugin-store'

let settingsStore: Store | null = null

export const getSettingsStore = async (): Promise<Store | null> => {
  if (settingsStore) return settingsStore
  try {
    settingsStore = await Store.load('settings.json')
    return settingsStore
  } catch (e) {
    console.warn('store init failed', e)
    settingsStore = null
    return null
  }
}

// Token helpers
export const getTokenFromStore = async (): Promise<string | null> => {
  const store = await getSettingsStore()
  if (!store) return null
  try { return (await store.get<string>('token')) ?? null } catch { return null }
}
export const setTokenToStore = async (token: string): Promise<void> => {
  const store = await getSettingsStore(); if (!store) return
  try { await store.set('token', token); await store.save() } catch { /* noop */ }
}

// Endpoint helpers
export const getEndpointFromStore = async (): Promise<string | null> => {
  const store = await getSettingsStore()
  if (!store) return null
  try { return (await store.get<string>('endpoint')) ?? null } catch { return null }
}
export const setEndpointToStore = async (endpoint: string): Promise<void> => {
  const store = await getSettingsStore(); if (!store) return
  try { await store.set('endpoint', endpoint); await store.save() } catch { /* noop */ }
}

// User info helpers
export const getUserInfoFromStore = async <T = any>(): Promise<T | null> => {
  const store = await getSettingsStore()
  if (!store) return null
  try { return (await store.get<T>('user_info')) ?? null } catch { return null }
}
export const setUserInfoToStore = async (info: any): Promise<void> => {
  const store = await getSettingsStore(); if (!store) return
  try { await store.set('user_info', info); await store.save() } catch { /* noop */ }
}

// SysInfo helpers
export const getSysInfoFromStore = async <T = any>(): Promise<T | null> => {
  const store = await getSettingsStore()
  if (!store) return null
  try {
    const info = await store.get<T>('sys_info')
    return (info as T) || null
  } catch (e) {
    console.warn('get sys_info failed', e)
    return null
  }
}

export const setSysInfoToStore = async (info: any): Promise<void> => {
  const store = await getSettingsStore()
  if (!store) return
  try {
    await store.set('sys_info', info)
    await store.save()
  } catch (e) {
    console.warn('set sys_info failed', e)
  }
}

// Local parse flag helpers
const LOCAL_PARSE_KEY = 'local_parse_enabled'
export const getLocalParseFlag = async (): Promise<boolean> => {
  const store = await getSettingsStore()
  if (!store) return false
  try {
    const v = await store.get<boolean>(LOCAL_PARSE_KEY)
    return !!v
  } catch (e) {
    console.warn('get local_parse_enabled failed', e)
    return false
  }
}

export const setLocalParseFlag = async (v: boolean): Promise<void> => {
  const store = await getSettingsStore()
  if (!store) return
  try {
    await store.set(LOCAL_PARSE_KEY, v)
    await store.save()
  } catch (e) {
    console.warn('set local_parse_enabled failed', e)
  }
}

export const clearStoreExceptEndpoint = async (): Promise<void> => {
  const store = await getSettingsStore(); if (!store) return
  try {
    const ep = await store.get<string>('endpoint')
    store.clear()
    if (ep) await store.set('endpoint', ep)
    await store.save()
  } catch (e) {
    console.warn('clear store except endpoint failed', e)
  }
}
