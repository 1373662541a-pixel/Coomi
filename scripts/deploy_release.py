#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Coomi-Android 发布部署脚本：
  1) 上传 APK + latest.json 到更新服务器 8.148.146.68
     /www/wwwroot/updates.septemc.com/coomi/android/
  2) 上传官网 index.html 到 /www/wwwroot/coomi.septemc.com/

用法:
  python scripts/deploy_release.py --ssh-config <连接配置文件> --apk <apk路径>
      [--latest <latest.json路径>] [--site-index <index.html路径>]

连接配置为 SSH-Agent 的 ssh-configs txt（含 服务器IP / SSH端口 / 用户名 / 密码）。
"""
import argparse
import re
import sys

import paramiko


def parse_ssh_config(path):
    """从 SSH-Agent 配置文件里解析 host/port/user/password。"""
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        text = f.read()
    ip = re.search(r"服务器IP:?\s*([\d.]+)", text)
    port = re.search(r"SSH端口:?\s*(\d+)", text)
    user = re.search(r"用户名:?\s*(\w+)", text)
    pwd = re.search(r"密码:?\s*(\S+)", text)
    if not (ip and pwd):
        raise SystemExit(f"无法从 {path} 解析连接信息")
    return {
        "host": ip.group(1),
        "port": int(port.group(1)) if port else 22,
        "user": user.group(1) if user else "root",
        "password": pwd.group(1).rstrip(),
    }


def connect(cfg):
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(
        cfg["host"], port=cfg["port"], username=cfg["user"],
        password=cfg["password"], timeout=20, banner_timeout=20,
        auth_timeout=20, look_for_keys=False, allow_agent=False,
    )
    return client


def run(client, command):
    stdin, stdout, stderr = client.exec_command(command, timeout=60)
    out = stdout.read().decode("utf-8", "replace")
    err = stderr.read().decode("utf-8", "replace")
    code = stdout.channel.recv_exit_status()
    return code, out, err


def upload(client, local, remote):
    sftp = client.open_sftp()
    try:
        sftp.put(local, remote)
    finally:
        sftp.close()
    print(f"  ↑ {local} -> {remote}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ssh-config", required=True, help="SSH-Agent 连接配置文件路径")
    ap.add_argument("--apk", required=True, help="APK 文件路径")
    ap.add_argument("--latest", help="latest.json 路径")
    ap.add_argument("--site-index", help="官网 index.html 路径")
    args = ap.parse_args()

    cfg = parse_ssh_config(args.ssh_config)
    print(f"连接 {cfg['user']}@{cfg['host']}:{cfg['port']} ...")
    client = connect(cfg)
    print("连接成功")

    base = "/www/wwwroot/updates.septemc.com/coomi/android"
    site_root = "/www/wwwroot/coomi.septemc.com"

    code, out, err = run(client, f"mkdir -p {base}")
    if code != 0:
        print("mkdir 失败:", err)
        sys.exit(1)

    upload(client, args.apk, f"{base}/{args.apk.replace(chr(92), '/').split('/')[-1]}")

    if args.latest:
        upload(client, args.latest, f"{base}/latest.json")

    if args.site_index:
        code, out, err = run(client, f"test -d {site_root}")
        if code != 0:
            print(f"警告：服务器上不存在 {site_root}，跳过官网 index.html 上传（{err.strip()}）")
        else:
            upload(client, args.site_index, f"{site_root}/index.html")

    code, out, err = run(
        client,
        f"ls -la {base} && echo '---' && sha256sum {base}/*.apk {base}/latest.json",
    )
    print(out)
    if err:
        print("stderr:", err)

    client.close()
    print("部署完成")


if __name__ == "__main__":
    main()
