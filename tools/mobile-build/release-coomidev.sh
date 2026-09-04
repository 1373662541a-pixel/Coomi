#!/bin/sh
# ---------------------------------------------------------------------------
# CoomiDev 一键发布脚本
#
# 原理：CoomiDev 的 APK 由云端 GitHub Actions（.github/workflows/build-coomidev.yml）
# 打包 + 自动发布 GitHub Release。只要把改动推到 main（且改动涉及构建相关路径），
# Actions 就会自动构建并发布新版 CoomiDev APK；App 内的更新检查（UpdateChecker）
# 检测到更高版本后会提示/自动下载更新 —— 即「推送 main = 云端出新版 = App 可自更新」。
#
# 本机是 ARM64，官方 Android NDK 没有 linux-aarch64 宿主工具链，本机打包需另装重型
# Build Kit；日常发版走云端 Actions 最省事。
#
# 用法：
#   ./release-coomidev.sh "提交说明"            # 只提交（不推送）
#   ./release-coomidev.sh "提交说明" --push     # 提交并推送 main，触发云端打包
#
# 说明：
#   - 自动暂存所有【已修改的跟踪文件】（git add -u），无需维护文件清单；
#     未跟踪的新文件不会被带入，请手动决定是否加入提交。
#   - 推送前先跑一遍引擎图片链路回归测试，确认没改坏。
# ---------------------------------------------------------------------------
set -eu

usage() {
    printf '%s\n' 'Usage: release-coomidev.sh "commit message" [--push]'
    exit 2
}

[ "$#" -ge 1 ] || usage
msg=$1
do_push=0
[ "${2:-}" = "--push" ] && do_push=1

# 定位仓库根
repo_root=$PWD
while [ "$repo_root" != / ]; do
    if [ -f "$repo_root/settings.gradle" ] && [ -d "$repo_root/apps/coomi-app" ]; then
        break
    fi
    repo_root=$(dirname "$repo_root")
done
[ -f "$repo_root/settings.gradle" ] || { printf '[error] 请在 Coomi 源码根目录内运行\n' >&2; exit 1; }
cd "$repo_root"

# 确保在 main 且没有遗留冲突
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = "main" ] || { printf '[error] 当前分支 %s，请先切到 main\n' "$branch" >&2; exit 1; }
if ! git diff --cached --quiet; then
    printf '[error] 暂存区已有未提交内容，请先处理\n' >&2
    exit 1
fi

# 快速回归：图片链路集成测试（无网络、毫秒级）
printf '[..] 运行图片链路回归测试\n'
if [ -d apps/coomi-rs ]; then
    (cd apps/coomi-rs && cargo test -q -p coomi-engine user_image_original_data_reaches_the_model_request) \
        || { printf '[error] 图片链路测试未通过，请先修复\n' >&2; exit 1; }
fi
printf '[ok] 图片链路测试通过\n'

# 暂存所有已修改的跟踪文件并提交
printf '[..] 提交改动\n'
git add -u
# 无改动可提交时报错退出
if git diff --cached --quiet; then
    printf '[error] 没有检测到改动（working tree 无已修改的跟踪文件）\n' >&2
    exit 1
fi
git commit -m "$msg"
printf '[ok] 已提交: %s\n' "$msg"

if [ "$do_push" = 1 ]; then
    printf '[..] 推送 main，触发云端打包\n'
    git push origin main
    printf '[ok] 已推送。构建完成后 App 内即可检测并更新到新版。\n'
else
    printf '[ok] 未推送。确认无误后执行：%s --push 触发云端打包\n' "$0"
fi