import { fetch } from "@tauri-apps/plugin-http";

type FtOptions = {
  method?: string;
  headers?: Record<string, string>;
  body?: any;
};

export const ft = async (url: string, options: FtOptions = {}) => {
  const token = localStorage.getItem("token");
  const endpoint = localStorage.getItem("endpoint") || "";
  const custHeaders: Record<string, string> = options.headers || {};
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...custHeaders,
  };
  if (token) headers["Authorization"] = token;
  const response = await fetch(endpoint + url, { ...options, headers });

  // 全局错误处理
  if (!response.ok) {
    // 可以统一处理 401、403 等
    console.error("Fetch error:", response.status);
  }

  return response.json(); // 或 response.text() / response.blob() 根据需求
};

export const ftget = (url: string, options: FtOptions = {}) =>
  ft(url, { ...options, method: "GET" });
export const ftpost = (url: string, body: any, options: FtOptions = {}) =>
  ft(url, { ...options, method: "POST", body: JSON.stringify(body) });
export const ftdelete = (url: string, options: FtOptions = {}) =>
  ft(url, { ...options, method: "DELETE" });
