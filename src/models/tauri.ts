import { invoke, convertFileSrc } from '@tauri-apps/api/core';

// 获取
export const load_avatar = async (path: string): Promise<string> => {
    const stripLongPathPrefix = path.replace(/^\\\\\?\\/, '');
    return invoke<string>('load_avatar', { path: stripLongPathPrefix })
}
// 提取微信数据
export const extract_wechat_keys = (): Promise<any> => {
    return invoke('extract_wechat_keys', { dataDir: null })
}

const stripLongPathPrefix = (p: string) => p.replace(/^\\\\\?\\/, '');
const isLocalPath = (p: string) => /^[a-zA-Z]:[\\/]/.test(p) || p.startsWith('\\\\') || p.startsWith('/');

// 转换图片
export const resolveAvatar = async (src: string): Promise<string> => {
  if (!src) return ''
  // handle url-like sources
  if (/^(https?:|data:|asset:|tauri:|file:)/i.test(src)) {
    if (/^file:\/\//i.test(src)) {
      const path = src.replace(/^file:\/\//i, '')
      try { 
        return convertFileSrc(stripLongPathPrefix(path)) 
      } catch { 
        return src;
      }
    }
    return src;
  }
  // handle plain local paths
  if (isLocalPath(src)) {
    try {
      return await invoke<string>('load_avatar', { path: stripLongPathPrefix(src) })
    } catch { return src }
  }
  console.log('resolve avatar', src);
  return src
}