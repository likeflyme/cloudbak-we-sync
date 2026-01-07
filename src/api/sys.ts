import { ftget } from "./api";

export const getSysInfo = () => {
    return ftget('/api/sys/sys-info');
}