package app.coomi;

import android.content.Context;

import com.termux.shared.termux.TermuxConstants;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

final class CoomiBackupArchive {
    private CoomiBackupArchive() {}

    static File create(Context context) throws Exception {
        File home = new File(CoomiConstants.COOMI_CONFIG_DIR);
        File virtualHome = new File(TermuxConstants.TERMUX_HOME_DIR_PATH);
        File archive = File.createTempFile("coomi-backup-", ".zip", context.getCacheDir());
        boolean complete = false;
        try (ZipOutputStream output = new ZipOutputStream(new FileOutputStream(archive))) {
            addDirectory(output, home, "coomi-home");
            addUserData(output, virtualHome, "user-data", true);
            addMcpFiles(output, new File(home, "config/mcp_servers.json"));
            complete = true;
        } finally {
            if (!complete) archive.delete();
        }
        verify(archive);
        return archive;
    }

    static void verify(File archive) throws Exception {
        int entries = 0;
        try (ZipInputStream input = new ZipInputStream(new FileInputStream(archive))) {
            while (input.getNextEntry() != null) {
                entries++;
                byte[] buffer = new byte[8192];
                while (input.read(buffer) >= 0) { /* validate every entry */ }
                input.closeEntry();
            }
        }
        if (entries == 0) throw new IllegalStateException("备份包为空");
    }

    private static void addDirectory(ZipOutputStream output, File directory, String prefix) throws Exception {
        File[] children = directory.listFiles();
        if (children == null) return;
        for (File child : children) {
            String name = prefix + "/" + child.getName();
            if (child.isDirectory()) addDirectory(output, child, name);
            else addFile(output, name, child);
        }
    }

    private static void addUserData(ZipOutputStream output, File directory, String prefix, boolean root) throws Exception {
        File[] children = directory.listFiles();
        if (children == null) return;
        for (File child : children) {
            String name = child.getName();
            if ((root && ".coomi".equals(name)) || isEnvironmentEntry(name)) continue;
            String entryName = prefix + "/" + name;
            if (child.isDirectory()) addUserData(output, child, entryName, false);
            else addFile(output, entryName, child);
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

    private static void addMcpFiles(ZipOutputStream output, File config) throws Exception {
        JSONObject root = readJson(config);
        JSONObject servers = root == null ? null : root.optJSONObject("servers");
        if (servers == null) return;
        Set<String> candidates = new LinkedHashSet<>();
        for (Iterator<String> names = servers.keys(); names.hasNext();) {
            JSONObject server = servers.optJSONObject(names.next());
            if (server == null) continue;
            candidates.add(server.optString("command", ""));
            JSONArray args = server.optJSONArray("args");
            if (args != null) {
                for (int index = 0; index < args.length(); index++) candidates.add(args.optString(index, ""));
            }
        }
        JSONArray manifest = new JSONArray();
        File termuxHome = new File(TermuxConstants.TERMUX_HOME_DIR_PATH).getCanonicalFile();
        int index = 0;
        for (String raw : candidates) {
            if (raw == null || raw.trim().isEmpty()) continue;
            File source = new File(raw.startsWith("~/") ? new File(termuxHome, raw.substring(2)).getPath() : raw);
            if (!source.exists()) continue;
            source = source.getCanonicalFile();
            if (!isInside(termuxHome, source)) continue;
            String archivePath = "mcp-files/files/" + index++;
            if (source.isDirectory()) addDirectory(output, source, archivePath);
            else addFile(output, archivePath, source);
            JSONObject item = new JSONObject();
            item.put("archive", archivePath);
            item.put("original", source.getAbsolutePath());
            item.put("directory", source.isDirectory());
            manifest.put(item);
        }
        output.putNextEntry(new ZipEntry("mcp-files/manifest.json"));
        output.write(manifest.toString(2).getBytes("UTF-8"));
        output.closeEntry();
    }

    private static boolean isInside(File base, File child) throws Exception {
        String basePath = base.getCanonicalPath();
        String childPath = child.getCanonicalPath();
        return childPath.equals(basePath) || childPath.startsWith(basePath + File.separator);
    }

    private static void addFile(ZipOutputStream output, String name, File source) throws Exception {
        if (!source.isFile()) return;
        output.putNextEntry(new ZipEntry(name));
        try (InputStream input = new FileInputStream(source)) {
            byte[] buffer = new byte[8192];
            int read;
            while ((read = input.read(buffer)) >= 0) output.write(buffer, 0, read);
        }
        output.closeEntry();
    }

    private static JSONObject readJson(File file) {
        if (!file.isFile()) return null;
        try (InputStream input = new FileInputStream(file)) {
            byte[] bytes = new byte[(int) file.length()];
            int offset = 0;
            while (offset < bytes.length) {
                int read = input.read(bytes, offset, bytes.length - offset);
                if (read < 0) break;
                offset += read;
            }
            return new JSONObject(new String(bytes, 0, offset, "UTF-8"));
        } catch (Exception ignored) {
            return null;
        }
    }
}
