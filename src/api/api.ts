import { fetch } from "@tauri-apps/plugin-http";

export const ft = async (url:string, options = {}) => {
    const token = localStorage.getItem('token');
    const endpoint = localStorage.getItem("endpoint") || "";
    const custHeaders = options['headers'] || {};
    const headers = {
        'Content-Type': 'application/json',
        ...custHeaders
    };
    headers['Authorization'] = token;
    const response = await fetch(endpoint + url, { ...options, headers });
  
    // 全局错误处理
    if (!response.ok) {
        // 可以统一处理 401、403 等
        console.error('Fetch error:', response.status);
    }

    return response.json(); // 或 response.text() / response.blob() 根据需求
}

export const ftget = (url:string, options = {}) => {
    return ft(url, { ...options, method: 'GET' });
}

export const ftpost = (url:string, body:any, options = {}) => {
    return ft(url, { ...options, method: 'POST', body: JSON.stringify(body) });
}

export const ftdelete = (url:string, options = {}) => {
    return ft(url, { ...options, method: 'DELETE' });
}
