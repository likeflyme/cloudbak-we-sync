import { fetch } from "@tauri-apps/plugin-http";
import router from "../router"; // 新增: 用于在403时跳转登录页
import { getTokenFromStore, getEndpointFromStore } from '@/common/store'

type FtOptions = {
  method?: string;
  headers?: Record<string, string>;
  body?: any;
};

export const ft = async (url: string, options: FtOptions = {}) => {
  // 优先从 store 获取，其次回退 localStorage
  const storeToken = await getTokenFromStore();
  const token = storeToken ?? localStorage.getItem("token") ?? undefined;
  const storeEndpoint = await getEndpointFromStore();
  const endpoint = (storeEndpoint ?? localStorage.getItem("endpoint")) || "";
  console.log(`Fetching ${options.method || 'GET'} ${endpoint + url}`);
  const custHeaders: Record<string, string> = options.headers || {};
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...custHeaders,
  };
  if (token) headers["Authorization"] = token;
  const response = await fetch(endpoint + url, { ...options, headers });

  // 全局错误处理
  if (response.status === 403) {
    console.warn("403 Forbidden: logging out");
    localStorage.removeItem("token");
    router.replace("/login");
    throw new Error("Forbidden");
  }
  if (!response.ok) {
    console.error("Fetch error:", response.status);
    throw new Error("HTTP " + response.status);
  }

  return response.json(); // 或 response.text() / response.blob() 根据需求
};

export const ftget = (url: string, options: FtOptions = {}) =>
  ft(url, { ...options, method: "GET" });
export const ftpost = (url: string, body: any = {}, options: FtOptions = {}) =>
  ft(url, { ...options, method: "POST", body: JSON.stringify(body) });
export const ftdelete = (url: string, options: FtOptions = {}) =>
  ft(url, { ...options, method: "DELETE" });
