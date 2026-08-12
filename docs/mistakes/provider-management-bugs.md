# Provider 管理已知问题

记录日期：2026-08-13
分支：`codex/feat/provider-config`
版本：`1.2.7` Debug 测试包

## BUG-001：覆盖安装后无法清理已保存的内置 Provider

- **复现条件**：设备上已有已保存的 Provider；使用新 APK 覆盖安装，不清除应用数据；进入该 Provider 的详情页。
- **操作步骤**：点击页面底部的“清空配置”。
- **预期结果**：删除该 Provider 的已保存配置，内置 Provider 行仍保留并恢复为“未配置”，随后返回供应商列表。
- **实际结果**：截图中的“清空配置”不可正常使用，点击后无法完成清理。
- **证据**：`C:\Users\12\AppData\Local\Temp\codex-clipboard-425c4a7f-1e76-4a2e-8757-ee92c4b273dd.png`
- **状态**：待定位、待修复。

## BUG-002：新建空 Provider 点击删除请求空 ID

- **复现条件**：点击“添加供应商”新建 Provider，不填写 API Key 和 API Base URL，也不保存配置。
- **操作步骤**：点击页面底部的“删除供应商”。
- **预期结果**：直接放弃并删除未保存的草稿，返回供应商列表；不应发送删除后端请求。
- **实际结果**：页面显示 `Error: DELETE /api/providers/ → 405`，请求路径缺少 Provider ID。
- **证据**：
  - `C:\Users\12\AppData\Local\Temp\codex-clipboard-1da24c87-4de2-4a47-99c6-f591babc924d.png`
  - `C:\Users\12\AppData\Local\Temp\codex-clipboard-a3d0ccef-b623-4f28-ba78-b5929afa8eb6.png`
- **状态**：待修复。

## 相关代码位置

- Provider 详情页删除/清空入口：`apps/web/src/views/ProviderDetailView.vue`
- Provider 删除客户端请求：`apps/web/src/stores/config.ts`
- Provider 删除后端路由：`apps/coomi-rs/ui/src/web.rs`
