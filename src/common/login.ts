import { getTokenFromStore, getEndpointFromStore, setTokenToStore } from "./store";

export const isLogin = () => {
    return !!token();
};

export const token = async () => {
    return await getTokenFromStore();
};

export const saveToken = async (token: string) => {
    await setTokenToStore(token);
}

export const removeToken = () => {
    localStorage.removeItem("token");
}

export const endpoint = async () => {
    return await getEndpointFromStore();
}