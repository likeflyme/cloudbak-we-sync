import { ftget, ftpost, ftdelete } from "./api";
import { PartialSession } from "@/models/session";


export const getSessions = () => {
    return ftget('/api/user/sys-sessions');
}

export const addSession = (sysSession: PartialSession) => {
    // sanitize xor_key: optional int; remove if empty string; convert hex/dec string to number
    const payload: any = { ...sysSession };
    if (payload.xor_key !== undefined) {
        const raw = payload.xor_key;
        if (raw === '' || raw === null) {
            delete payload.xor_key;
        } else if (typeof raw === 'string') {
            const trimmed = raw.trim();
            if (trimmed === '') {
                delete payload.xor_key;
            } else {
                let num: number | null = null;
                if (/^0x[0-9a-fA-F]+$/.test(trimmed)) {
                    num = parseInt(trimmed.slice(2), 16);
                } else if (/^[0-9]+$/.test(trimmed)) {
                    num = parseInt(trimmed, 10);
                } else if (/^[0-9a-fA-F]+$/.test(trimmed) && trimmed.length <= 8) {
                    // treat as hex if contains any a-f characters, else decimal
                    num = parseInt(trimmed, /[a-fA-F]/.test(trimmed) ? 16 : 10);
                }
                if (num !== null && Number.isFinite(num)) {
                    payload.xor_key = num;
                } else {
                    // invalid -> drop to satisfy optional behavior
                    delete payload.xor_key;
                }
            }
        } else if (typeof raw === 'number') {
            if (!Number.isFinite(raw)) delete payload.xor_key;
        } else {
            delete payload.xor_key;
        }
    }
    console.log('Adding session payload:', payload);
    return ftpost(`/api/user/sys-session`, payload);
}

export const deleteSession = (sessionId: number) => {
    return ftdelete(`/api/user/sys-session/${sessionId}`);
}