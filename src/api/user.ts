import { ftget, ftpost } from "./api";


export const getSessions = () => {
    return ftget('/api/user/sys-sessions');
}

export const addSession = (sysSession: any) => {
    return ftpost(`/api/user/add-sys-session`, sysSession);
}