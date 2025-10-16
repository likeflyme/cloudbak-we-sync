class LoginModel {
    username: string;
    password: string;
    endpoint: string;
    constructor(username: string, password: string, endpoint: string) {
        this.username = username;
        this.password = password;
        this.endpoint = endpoint;
    }
}