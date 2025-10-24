import { ftpost } from "./api";

export const decrypt = (sessionId: number) => {
    return ftpost(`/api/task/single-decrypt/${sessionId}/false`);
}