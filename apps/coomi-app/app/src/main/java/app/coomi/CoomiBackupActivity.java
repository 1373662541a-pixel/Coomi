package app.coomi;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ContentValues;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.provider.MediaStore;
import android.view.View;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

import com.termux.R;
import com.termux.shared.termux.TermuxConstants;

/**
 * 备份与导入二级页面。
 * 备份：会话记录 + 已安装 Skill 完整内容 + MCP 配置 + Provider 配置（密钥打码）打包成 zip。
 * 导入：选择备份压缩包，自动恢复会话历史、MCP 与 Skill，并提醒环境配置需重新确认。
 */
public class CoomiBackupActivity extends Activity {

    private static final int REQ_IMPORT = 4001;
    private static final int REQ_EXPORT = 4002;

    private TextView mStatusText;
    private String mAppliedThemeMode;
    private File mPendingBackup;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        CoomiTheme.applyPageTheme(this);
        super.onCreate(savedInstanceState);
        mAppliedThemeMode = CoomiTheme.getMode(this);
        setContentView(R.layout.activity_coomi_backup);
        CoomiTheme.applyPageSystemBars(this);

        mStatusText = findViewById(R.id.txt_backup_status);

        findViewById(R.id.btn_backup_back).setOnClickListener(v -> finish());
        findViewById(R.id.btn_backup_action).setOnClickListener(v -> backupData());
        findViewById(R.id.btn_backup_import).setOnClickListener(v -> pickBackupZip());
    }

    private void pickBackupZip() {
        Intent intent = new Intent(Intent.ACTION_GET_CONTENT);
        intent.setType("application/zip");
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        try {
            startActivityForResult(intent, REQ_IMPORT);
        } catch (Exception e) {
            Toast.makeText(this,
                getString(R.string.coomi_backup_import_failed, e.getMessage()), Toast.LENGTH_LONG).show();
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == REQ_IMPORT && resultCode == RESULT_OK && data != null && data.getData() != null) {
            importBackup(data.getData());
        } else if (requestCode == REQ_EXPORT && resultCode == RESULT_OK && data != null && data.getData() != null && mPendingBackup != null) {
            final File source = mPendingBackup;
            final Uri destination = data.getData();
            new Thread(() -> {
                try (InputStream in = new FileInputStream(source); OutputStream out = getContentResolver().openOutputStream(destination)) {
                    if (out == null) throw new Exception("无法打开保存位置");
                    copyStream(in, out);
                    runOnUiThread(() -> Toast.makeText(this, R.string.coomi_dash_backup_done, Toast.LENGTH_LONG).show());
                } catch (Exception e) {
                    runOnUiThread(() -> Toast.makeText(this, getString(R.string.coomi_dash_backup_failed, e.getMessage()), Toast.LENGTH_LONG).show());
                } finally {
                    source.delete();
                    mPendingBackup = null;
                }
            }).start();
        }
    }

    // ==================== 备份 ====================

    /** 备份：会话 + Skill 内容 + MCP/Provider 配置 + 清单，保存到下载目录。 */
    private void backupData() {
        Toast.makeText(this, R.string.coomi_dash_backup_starting, Toast.LENGTH_SHORT).show();
        new Thread(() -> {
            try {
                File home = new File(CoomiConstants.COOMI_CONFIG_DIR);
                File virtualHome = new File(TermuxConstants.TERMUX_HOME_DIR_PATH);
                File configDir = new File(home, "config");

                File zip = File.createTempFile("coomi-backup-", ".zip", getCacheDir());
                try (ZipOutputStream zos = new ZipOutputStream(new FileOutputStream(zip))) {
                    // Back up Coomi data/configuration and user-created HOME files, but not
                    // package caches, language runtimes, virtual environments, or shell state.
                    addDirRecursive(zos, home, "coomi-home");
                    addUserDataRecursive(zos, virtualHome, "user-data", true);
                    addMcpImplementationFiles(zos, new File(configDir, "mcp_servers.json"));
                }

                String stamp = new SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(new Date());
                runOnUiThread(() -> {
                    mPendingBackup = zip;
                    Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                    intent.addCategory(Intent.CATEGORY_OPENABLE);
                    intent.setType("application/zip");
                    intent.putExtra(Intent.EXTRA_TITLE, "coomi-backup-" + stamp + ".zip");
                    startActivityForResult(intent, REQ_EXPORT);
                });
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this,
                    getString(R.string.coomi_dash_backup_failed, e.getMessage()), Toast.LENGTH_LONG).show());
            }
        }).start();
    }

    private void addSessions(ZipOutputStream zos, File sessionsDir) throws Exception {
        if (!sessionsDir.isDirectory()) return;
        File[] files = sessionsDir.listFiles((d, n) -> n.endsWith(".json"));
        if (files == null) return;
        for (File f : files) {
            if (!f.isFile()) continue;
            zos.putNextEntry(new ZipEntry("sessions/" + f.getName()));
            try (InputStream in = new FileInputStream(f)) {
                copyStream(in, zos);
            }
            zos.closeEntry();
        }
    }

    /** 打包已安装 Skill 的完整内容（SKILL.md 等），导入时可直接还原。 */
    private void addSkills(ZipOutputStream zos, File skillsDir) throws Exception {
        if (!skillsDir.isDirectory()) return;
        File[] dirs = skillsDir.listFiles(File::isDirectory);
        if (dirs == null) return;
        for (File s : dirs) {
            addDirRecursive(zos, s, "skills/" + s.getName());
        }
    }

    private void addDirRecursive(ZipOutputStream zos, File dir, String prefix) throws Exception {
        File[] children = dir.listFiles();
        if (children == null) return;
        for (File c : children) {
            String entryName = prefix + "/" + c.getName();
            if (c.isDirectory()) {
                addDirRecursive(zos, c, entryName);
            } else {
                zos.putNextEntry(new ZipEntry(entryName));
                try (InputStream in = new FileInputStream(c)) {
                    copyStream(in, zos);
                }
                zos.closeEntry();
            }
        }
    }

    private void addUserDataRecursive(ZipOutputStream zos, File dir, String prefix, boolean root) throws Exception {
        File[] children = dir.listFiles();
        if (children == null) return;
        for (File child : children) {
            String name = child.getName();
            if ((root && ".coomi".equals(name)) || isEnvironmentEntry(name)) continue;
            String entryName = prefix + "/" + name;
            if (child.isDirectory()) addUserDataRecursive(zos, child, entryName, false);
            else addFileEntry(zos, entryName, child);
        }
    }

    private static boolean isEnvironmentEntry(String name) {
        String lower = name.toLowerCase(Locale.US);
        return lower.equals(".cache") || lower.equals(".npm") || lower.equals(".pnpm-store")
            || lower.equals(".cargo") || lower.equals(".rustup") || lower.equals(".gradle")
            || lower.equals("node_modules") || lower.equals("venv") || lower.equals(".venv")
            || lower.equals("env") || lower.equals("__pycache__") || lower.equals("tmp")
            || lower.equals(".tmp") || lower.equals(".bash_history") || lower.equals(".zsh_history")
            || lower.equals(".bashrc") || lower.equals(".zshrc") || lower.equals(".profile");
    }

    @Override
    protected void onResume() {
        super.onResume();
        String current = CoomiTheme.getMode(this);
        if (mAppliedThemeMode != null && !mAppliedThemeMode.equals(current)) {
            recreate();
            return;
        }
        CoomiTheme.applyPageSystemBars(this);
    }

    /** Includes local MCP scripts/projects referenced by command or args when they live in Termux HOME. */
    private void addMcpImplementationFiles(ZipOutputStream zos, File config) throws Exception {
        JSONObject root = readJsonObject(config);
        JSONObject servers = root == null ? null : root.optJSONObject("servers");
        if (servers == null) return;
        Set<String> candidates = new LinkedHashSet<>();
        for (Iterator<String> names = servers.keys(); names.hasNext(); ) {
            JSONObject server = servers.optJSONObject(names.next());
            if (server == null) continue;
            candidates.add(server.optString("command", ""));
            JSONArray args = server.optJSONArray("args");
            if (args != null) {
                for (int index = 0; index < args.length(); index++) candidates.add(args.optString(index, ""));
            }
        }
        JSONArray manifest = new JSONArray();
        int index = 0;
        File termuxHome = new File(TermuxConstants.TERMUX_HOME_DIR_PATH).getCanonicalFile();
        for (String raw : candidates) {
            if (raw == null || raw.trim().isEmpty()) continue;
            String expanded = raw.startsWith("~/") ? new File(termuxHome, raw.substring(2)).getPath() : raw;
            File source = new File(expanded);
            if (!source.exists()) continue;
            source = source.getCanonicalFile();
            if (!isInside(termuxHome, source)) continue;
            String archive = "mcp-files/files/" + index++;
            addPathRecursive(zos, source, archive);
            JSONObject item = new JSONObject();
            item.put("archive", archive);
            item.put("original", source.getAbsolutePath());
            item.put("directory", source.isDirectory());
            manifest.put(item);
        }
        addTextEntry(zos, "mcp-files/manifest.json", manifest.toString(2));
    }

    private void addPathRecursive(ZipOutputStream zos, File source, String prefix) throws Exception {
        if (source.isDirectory()) addDirRecursive(zos, source, prefix);
        else addFileEntry(zos, prefix, source);
    }

    private static boolean isInside(File base, File child) throws Exception {
        String basePath = base.getCanonicalPath();
        String childPath = child.getCanonicalPath();
        return childPath.equals(basePath) || childPath.startsWith(basePath + File.separator);
    }

    private void addFileEntry(ZipOutputStream zos, String name, File f) throws Exception {
        if (!f.isFile()) return;
        zos.putNextEntry(new ZipEntry(name));
        try (InputStream in = new FileInputStream(f)) {
            copyStream(in, zos);
        }
        zos.closeEntry();
    }

    private void addTextEntry(ZipOutputStream zos, String name, String content) throws Exception {
        zos.putNextEntry(new ZipEntry(name));
        byte[] bytes = content.getBytes("UTF-8");
        zos.write(bytes, 0, bytes.length);
        zos.closeEntry();
    }

    // ==================== 导入 ====================

    /** 导入备份：解压到缓存目录，恢复会话 / MCP / Skill，弹窗总结并提醒环境配置。 */
    private void importBackup(Uri uri) {
        Toast.makeText(this, R.string.coomi_backup_importing, Toast.LENGTH_SHORT).show();
        mStatusText.setVisibility(View.VISIBLE);
        mStatusText.setText(R.string.coomi_backup_importing);

        new Thread(() -> {
            File tmp = new File(getCacheDir(), "coomi-import-" + System.currentTimeMillis());
            tmp.mkdirs();
            try (InputStream in = getContentResolver().openInputStream(uri);
                 ZipInputStream zis = new ZipInputStream(new BufferedInputStream(in))) {
                ZipEntry entry;
                while ((entry = zis.getNextEntry()) != null) {
                    if (entry.isDirectory()) continue;
                    File out = safeResolve(tmp, entry.getName());
                    if (out == null) continue;
                    out.getParentFile().mkdirs();
                    try (OutputStream os = new BufferedOutputStream(new FileOutputStream(out))) {
                        copyStream(zis, os);
                    }
                    zis.closeEntry();
                }
            } catch (Exception e) {
                runOnUiThread(() -> {
                    mStatusText.setVisibility(View.GONE);
                    Toast.makeText(this,
                        getString(R.string.coomi_backup_import_failed, e.getMessage()), Toast.LENGTH_LONG).show();
                });
                return;
            }

            int[] counts = restore(tmp);
            runOnUiThread(() -> {
                mStatusText.setVisibility(View.GONE);
                showImportResult(counts);
            });
        }).start();
    }

    /** 恢复备份内容到 ~/.coomi，返回 [会话数, MCP 数, Skill 数]。 */
    private int[] restore(File tmp) {
        int sessions = 0, mcps = 0, skills = 0;
        File home = new File(CoomiConstants.COOMI_CONFIG_DIR);
        File sessionsDir = new File(home, "sessions");
        File skillsDir = new File(home, "skills");
        File configDir = new File(home, "config");

        // Current backup format: restore the complete .coomi tree first.
        File completeHome = new File(tmp, "coomi-home");
        if (completeHome.isDirectory()) {
            try { copyRecursive(completeHome, home); } catch (Exception ignored) { }
        }
        File completeVirtualHome = new File(tmp, "virtual-home");
        if (completeVirtualHome.isDirectory()) {
            try { copyRecursive(completeVirtualHome, new File(TermuxConstants.TERMUX_HOME_DIR_PATH)); } catch (Exception ignored) { }
        }
        File userData = new File(tmp, "user-data");
        if (userData.isDirectory()) {
            try { copyRecursive(userData, new File(TermuxConstants.TERMUX_HOME_DIR_PATH)); } catch (Exception ignored) { }
        }

        // 1) 会话历史
        File[] sessFiles = new File(tmp, "sessions").listFiles((d, n) -> n.endsWith(".json"));
        if (sessFiles != null && sessFiles.length > 0) {
            sessionsDir.mkdirs();
            for (File f : sessFiles) {
                try {
                    copyFile(f, new File(sessionsDir, f.getName()));
                    sessions++;
                } catch (Exception ignored) {
                }
            }
        }

        // 2) Skill 完整内容
        File[] skillDirs = new File(tmp, "skills").listFiles(File::isDirectory);
        if (skillDirs != null && skillDirs.length > 0) {
            skillsDir.mkdirs();
            for (File s : skillDirs) {
                try {
                    File dest = new File(skillsDir, s.getName());
                    deleteRecursive(dest);
                    copyRecursive(s, dest);
                    skills++;
                } catch (Exception ignored) {
                }
            }
        }

        // 3) MCP 配置
        File mcpIn = new File(tmp, "config/mcp_servers.json");
        if (mcpIn.isFile()) {
            try {
                configDir.mkdirs();
                copyFile(mcpIn, new File(configDir, "mcp_servers.json"));
                mcps = countServers(mcpIn);
            } catch (Exception ignored) {
            }
        }

        // 4) Provider 配置（密钥打码版，导入后提醒重新配置）
        File provIn = new File(tmp, "config/providers.json");
        if (provIn.isFile()) {
            try {
                configDir.mkdirs();
                copyFile(provIn, new File(configDir, "providers.json"));
            } catch (Exception ignored) {
            }
        }
        restoreMcpImplementationFiles(tmp);
        File[] restoredSessions = sessionsDir.listFiles((d, n) -> n.endsWith(".json"));
        if (restoredSessions != null) sessions = restoredSessions.length;
        File[] restoredSkills = skillsDir.listFiles(File::isDirectory);
        if (restoredSkills != null) skills = restoredSkills.length;
        File restoredMcp = new File(configDir, "mcp_servers.json");
        if (restoredMcp.isFile()) mcps = countServers(restoredMcp);
        return new int[]{sessions, mcps, skills};
    }

    private void restoreMcpImplementationFiles(File tmp) {
        File manifestFile = new File(tmp, "mcp-files/manifest.json");
        if (!manifestFile.isFile()) return;
        try {
            JSONArray manifest = new JSONArray(readFile(manifestFile));
            File termuxHome = new File(TermuxConstants.TERMUX_HOME_DIR_PATH).getCanonicalFile();
            for (int index = 0; index < manifest.length(); index++) {
                JSONObject item = manifest.optJSONObject(index);
                if (item == null) continue;
                File source = safeResolve(tmp, item.optString("archive", ""));
                File destination = new File(item.optString("original", "")).getCanonicalFile();
                if (source == null || !source.exists() || !isInside(termuxHome, destination)) continue;
                copyRecursive(source, destination);
            }
        } catch (Exception ignored) { }
    }

    /** 防 zip-slip：仅允许解压到 tmp 目录内部。 */
    private File safeResolve(File base, String name) {
        try {
            File out = new File(base, name);
            String canonical = out.getCanonicalPath();
            String basePath = base.getCanonicalPath();
            if (!canonical.startsWith(basePath + File.separator) && !canonical.equals(basePath)) return null;
            return out;
        } catch (Exception e) {
            return null;
        }
    }

    private int countServers(File mcpJson) {
        try {
            JSONObject root = new JSONObject(readFile(mcpJson));
            JSONObject servers = root.optJSONObject("servers");
            return servers == null ? 0 : servers.length();
        } catch (Exception e) {
            return 0;
        }
    }

    private void showImportResult(int[] counts) {
        String result = getString(R.string.coomi_backup_import_result, counts[0], counts[1], counts[2]);
        String msg = result + "\n\n" + getString(R.string.coomi_backup_hint) + "\n"
            + getString(R.string.coomi_backup_restart_hint);
        new AlertDialog.Builder(this)
            .setTitle(R.string.coomi_backup_import_done)
            .setMessage(msg)
            .setPositiveButton(R.string.coomi_backup_ok, null)
            .show();
    }

    // ==================== 清单 ====================

    /** 人类可读的环境配置清单。 */
    private String buildEnvInventory(File configDir, File skillsDir) {
        StringBuilder sb = new StringBuilder();
        sb.append("Coomi 环境配置备份\n");
        sb.append("生成时间：").append(new SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(new Date())).append('\n');
        sb.append("应用版本：").append(UpdateChecker.currentVersionCode(this)).append("\n\n");

        sb.append("== 已安装 MCP Server ==\n");
        JSONObject mcp = readJsonObject(new File(configDir, "mcp_servers.json"));
        JSONObject servers = mcp == null ? null : mcp.optJSONObject("servers");
        if (servers != null && servers.length() > 0) {
            for (Iterator<String> it = servers.keys(); it.hasNext(); ) {
                String name = it.next();
                JSONObject s = servers.optJSONObject(name);
                sb.append("- ").append(name);
                if (s != null) {
                    sb.append(" | 启用: ").append(s.optBoolean("enabled", true) ? "是" : "否");
                    String cmd = s.optString("command", s.optString("url", ""));
                    if (!cmd.isEmpty()) sb.append(" | 命令/地址: ").append(cmd);
                }
                sb.append('\n');
            }
        } else {
            sb.append("（无）\n");
        }
        sb.append('\n');

        sb.append("== 已安装 Skill ==\n");
        File[] skillDirs = skillsDir.isDirectory() ? skillsDir.listFiles(File::isDirectory) : null;
        if (skillDirs != null && skillDirs.length > 0) {
            for (File s : skillDirs) {
                sb.append("- ").append(s.getName()).append('\n');
                File meta = new File(s, "SKILL.md");
                if (meta.isFile()) {
                    String first = firstNonEmptyLine(meta);
                    if (first != null) sb.append("  简介: ").append(first).append('\n');
                }
            }
        } else {
            sb.append("（无）\n");
        }
        sb.append('\n');

        sb.append("== 已配置 Provider ==\n");
        JSONObject prov = readJsonObject(new File(configDir, "providers.json"));
        JSONObject providers = prov == null ? null : prov.optJSONObject("providers");
        if (providers != null && providers.length() > 0) {
            for (Iterator<String> it = providers.keys(); it.hasNext(); ) {
                String id = it.next();
                JSONObject p = providers.optJSONObject(id);
                if (p == null) continue;
                sb.append("- ").append(id);
                sb.append(" | 模型: ").append(p.optString("model", "?"));
                String key = p.optString("api_key", p.optString("key", ""));
                sb.append(" | Key: ").append(maskKey(key)).append('\n');
            }
        } else {
            sb.append("（无）\n");
        }
        return sb.toString();
    }

    /** 结构化的环境配置清单（MCP / Skill 全量，Provider 密钥打码）。 */
    private String buildEnvInventoryJson(File configDir, File skillsDir) {
        try {
            JSONObject root = new JSONObject();
            root.put("created_at", new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.US).format(new Date()));
            root.put("app_version", UpdateChecker.currentVersionCode(this));
            JSONObject mcp = readJsonObject(new File(configDir, "mcp_servers.json"));
            root.put("mcp_servers", mcp == null ? new JSONObject() : mcp);

            JSONObject prov = readJsonObject(new File(configDir, "providers.json"));
            JSONObject provDoc = new JSONObject();
            if (prov != null) provDoc.put("active", prov.optString("active", ""));
            JSONObject masked = new JSONObject();
            JSONObject providers = prov == null ? null : prov.optJSONObject("providers");
            if (providers != null) {
                for (Iterator<String> it = providers.keys(); it.hasNext(); ) {
                    String id = it.next();
                    JSONObject p = providers.optJSONObject(id);
                    if (p == null) continue;
                    JSONObject copy = new JSONObject();
                    copy.put("model", p.optString("model", ""));
                    copy.put("api_key", maskKey(p.optString("api_key", p.optString("key", ""))));
                    masked.put(id, copy);
                }
            }
            provDoc.put("providers", masked);
            root.put("providers", provDoc);

            JSONArray skills = new JSONArray();
            File[] skillDirs = skillsDir.isDirectory() ? skillsDir.listFiles(File::isDirectory) : null;
            if (skillDirs != null) {
                for (File s : skillDirs) {
                    JSONObject item = new JSONObject();
                    item.put("name", s.getName());
                    File meta = new File(s, "SKILL.md");
                    String first = meta.isFile() ? firstNonEmptyLine(meta) : null;
                    item.put("summary", first == null ? "" : first);
                    skills.put(item);
                }
            }
            root.put("skills", skills);
            return root.toString(2);
        } catch (Exception e) {
            return "{}";
        }
    }

    private String maskKey(String key) {
        if (key == null || key.isEmpty()) return "（未设置）";
        if (key.length() <= 8) return "****";
        return key.substring(0, 4) + "****" + key.substring(key.length() - 4);
    }

    private JSONObject readJsonObject(File f) {
        if (!f.isFile()) return null;
        try {
            return new JSONObject(readFile(f));
        } catch (Exception e) {
            return null;
        }
    }

    private String readFile(File f) throws Exception {
        try (InputStream in = new FileInputStream(f)) {
            java.io.ByteArrayOutputStream buffer = new java.io.ByteArrayOutputStream();
            byte[] chunk = new byte[8192];
            int n;
            while ((n = in.read(chunk)) >= 0) buffer.write(chunk, 0, n);
            return new String(buffer.toByteArray(), "UTF-8");
        }
    }

    private String firstNonEmptyLine(File f) {
        try (java.io.BufferedReader reader = new java.io.BufferedReader(new java.io.FileReader(f))) {
            String line;
            while ((line = reader.readLine()) != null) {
                String t = line.trim();
                if (!t.isEmpty() && !t.startsWith("#")) return t;
            }
        } catch (Exception ignored) {
        }
        return null;
    }

    // ==================== 文件工具 ====================

    /** 保存到公共下载目录；成功返回可读路径，失败返回 null。 */
    private String saveToDownloads(File src, String displayName) {
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                // Android 10+：MediaStore 写入 Downloads，无需额外权限。
                ContentValues values = new ContentValues();
                values.put(MediaStore.Downloads.DISPLAY_NAME, displayName);
                values.put(MediaStore.Downloads.MIME_TYPE, "application/zip");
                values.put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS);
                Uri uri = getContentResolver().insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values);
                if (uri == null) return null;
                try (OutputStream out = getContentResolver().openOutputStream(uri);
                     InputStream in = new FileInputStream(src)) {
                    copyStream(in, out);
                }
                return "Download/" + displayName;
            }
            // Android 9-：公共下载目录（需要 WRITE_EXTERNAL_STORAGE）。
            File dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS);
            if (dir == null) return null;
            File out = new File(dir, displayName);
            try (OutputStream os = new FileOutputStream(out);
                 InputStream in = new FileInputStream(src)) {
                copyStream(in, os);
            }
            return out.getAbsolutePath();
        } catch (Exception e) {
            return null;
        }
    }

    private static void copyStream(InputStream in, OutputStream out) throws Exception {
        byte[] buf = new byte[65536];
        int n;
        while ((n = in.read(buf)) >= 0) out.write(buf, 0, n);
    }

    private static void copyFile(File src, File dst) throws Exception {
        try (InputStream in = new FileInputStream(src);
             OutputStream out = new FileOutputStream(dst)) {
            copyStream(in, out);
        }
    }

    private static void copyRecursive(File src, File dst) throws Exception {
        if (src.isDirectory()) {
            if (!dst.exists() && !dst.mkdirs()) throw new Exception("无法创建目录 " + dst);
            File[] children = src.listFiles();
            if (children != null) {
                for (File c : children) copyRecursive(c, new File(dst, c.getName()));
            }
        } else {
            copyFile(src, dst);
        }
    }

    private static void deleteRecursive(File f) {
        if (f.isDirectory()) {
            File[] children = f.listFiles();
            if (children != null) for (File c : children) deleteRecursive(c);
        }
        f.delete();
    }
}
