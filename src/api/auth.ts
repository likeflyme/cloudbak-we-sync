import { fetch } from "@tauri-apps/plugin-http";

export const login = async (data: LoginModel) => {
    const endpoint = data.endpoint;
    let formData = new FormData();
    formData.append('username', data.username);
    formData.append('password', data.password);
    formData.append('captcha', '');
    return fetch(endpoint + '/api/auth/token', {
        method: 'POST',
        body: formData
    });
}