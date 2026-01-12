import { fetch } from "@tauri-apps/plugin-http";

export interface LoginModel { endpoint: string; username: string; password: string }

export const login = async (data: LoginModel) => {
    const endpoint = data.endpoint;
    let formData = new FormData();
    formData.append('username', data.username);
    formData.append('password', data.password);
    formData.append('captcha', '');
    const url = endpoint + '/api/auth/token';
    return fetch(url, {
        method: 'POST',
        body: formData
    });
}

export const me = async (endpoint: string, token: string) => {
    return fetch(endpoint + '/api/auth/me', {
        method: 'GET',
        headers: {
            'Authorization': token
        }
    });
}