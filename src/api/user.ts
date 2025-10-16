import { ftget, ftpost, ftdelete } from "./api";
import { PartialSession } from "@/models/session";


export const getSessions = () => {
    return ftget('/api/user/sys-sessions');
}

export const addSession = (sysSession: PartialSession) => {
    return ftpost(`/api/user/sys-session`, sysSession);
}

export const deleteSession = (sessionId: number) => {
    return ftdelete(`/api/user/sys-session/${sessionId}`);
}