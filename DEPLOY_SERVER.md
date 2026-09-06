# Coomi 引擎 · Linux 服务器部署指南

> 适用于在自建 Linux 服务器上以「浏览器可访问的 Web 服务」方式运行 Coomi 引擎，
> 对外通过 Nginx + HTTPS 反向代理。Android 本机用法不受影响。

## 0. 部署架构

```
浏览器 ──HTTPS──> Nginx(443) ──http/ws──> coomi serve (0.0.0.0:8765)
                        │
                        └─ 静态前端（coomi serve 的 --static-dir，或由 Nginx 托管）
```

- 模型请求由**服务器**发出（更稳、可用官方稳定 key）。
- 服务器模式下 Agent 的文件 / Shell / 技能 / 进程类工具作用于**服务器文件系统**，
  不再是手机。

## 1. 构建二进制

```bash
cd apps/coomi-rs
cargo build --release -p coomi        # 产物：apps/coomi-rs/target/release/coomi
# 或整个 workspace 里已含，按需拷贝到服务器
```

## 2. 在服务器准备目录结构

```bash
sudo mkdir -p /opt/coomi/{home,work,web}
sudo chown -R $(whoami):$(whoami) /opt/coomi
```

把数据从手机迁移过去（重点：**providers.json 里含 API key**，务必走安全通道）：

```bash
# 数据目录（配置 / providers.json / skills / agents 等）
rsync -a /data/data/com.coomidev.android/files/home/.coomi/  /opt/coomi/home/
# 技能目录（如需要随引擎部署）
rsync -a /data/data/com.coomidev.android/files/home/skills/ /opt/coomi/home/skills/
# 前端 build（apps/web 构建产物）
rsync -a /data/data/com.coomidev.android/files/web/         /opt/coomi/web/
```

> 若 providers.json 的 key 是「手机/聚合转售」专用，建议在服务器上**换成官方稳定 key**
> 并把 base_url 指到官方，避免继续受转售服务抖动影响。

## 3. 生成强随机 token

```bash
openssl rand -hex 32   # 记下来，写进 systemd 环境变量
```

## 4. systemd 服务

`/etc/systemd/system/coomi.service`：

```ini
[Unit]
Description=Coomi engine
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=coomi
WorkingDirectory=/opt/coomi/work
Environment=COOMI_HOME=/opt/coomi/home
Environment=COOMI_TOKEN=__REPLACE_WITH_openssl_rand_hex_32__
ExecStart=/opt/coomi/coomi serve \
    --host 0.0.0.0 \
    --port 8765 \
    --home /opt/coomi/home \
    --cwd  /opt/coomi/work \
    --static-dir /opt/coomi/web \
    --token "${COOMI_TOKEN}" \
    --allow-origin https://coomi.example.com
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

> 关键点：
> - `--host 0.0.0.0` 让引擎可被 Nginx 反代访问（否则只听 127.0.0.1）。
> - `--allow-origin https://你的域名` 可重复传多个；CORS 和 WebSocket 握手只放行这里的域名 + loopback。
> - `--token` 必须设，否则 `/api/*` 和 `/ws/*` 对外无保护。
> - 单独跑一个低权限 `coomi` 系统用户，别用 root。

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now coomi
sudo systemctl status coomi
sudo journalctl -u coomi -f          # 看日志
```

## 5. Nginx HTTPS 反向代理

```nginx
server {
    listen 443 ssl http2;
    server_name coomi.example.com;

    ssl_certificate     /etc/letsencrypt/live/coomi.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/coomi.example.com/privkey.pem;

    client_max_body_size 25m;   # 需容纳含大图的消息（载荷守护阈值内）

    # WebSocket 升级头
    location /ws/ {
        proxy_pass http://127.0.0.1:8765;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 3600s;
    }

    # 引擎 API（转发到引擎，前端由引擎提供）
    location / {
        proxy_pass http://127.0.0.1:8765;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_read_timeout 600s;
    }
}

# 可选：HTTP -> HTTPS 跳转
server {
    listen 80;
    server_name coomi.example.com;
    return 301 https://$host$request_uri;
}
```

证书可用 certbot 一键签发：

```bash
sudo apt install certbot python3-certbot-nginx
sudo certbot --nginx -d coomi.example.com
```

## 6. 浏览器访问 / 验证

- 打开 `https://coomi.example.com`，首次会用 `--token` 作为访问凭据。
- 若自定义 `--token`，浏览器端登录或请求头 `Authorization: Bearer <token>`。

## 7. 安全清单（务必逐条做）

- [ ] **`--token` 用强随机值**，勿默认/空。
- [ ] **`--allow-origin` 只填你的真实域名**，不含泛域名。
- [ ] 引擎端口 8765 只对 Nginx 开放：`sudo ufw allow from 127.0.0.1 to any port 8765`（默认 DENY 外网直达）。
- [ ] **防火墙仅放行 80/443**。
- [ ] 服务器用**独立 `coomi` 低权限账号**，`--cwd` 指向专用目录。
- [ ] `providers.json`（含 API key）权限收紧：`chmod 600 /opt/coomi/home/config/providers.json`。
- [ ] HTTPS 证书有效期监控 / 自动续期。
- [ ] 及时更新引擎版本（拉取仓库新提交重新构建）。

## 8. 常用运维

```bash
sudo systemctl restart coomi
sudo journalctl -u coomi -n 200 -f
```

## 9. 回滚 / Android 不受影响

- 引擎的 `--host` 默认仍是 `127.0.0.1`，Android 本机用法零改动。
- 若服务器部署出问题，手机本地 Coomi 照常可用（无需依赖云服务器）。
