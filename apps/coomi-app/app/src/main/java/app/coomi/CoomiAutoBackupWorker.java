package app.coomi;

import android.content.Context;
import android.content.SharedPreferences;
import android.net.Uri;

import androidx.annotation.NonNull;
import androidx.documentfile.provider.DocumentFile;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.TimeUnit;
import java.util.zip.ZipInputStream;

public final class CoomiAutoBackupWorker extends Worker {
    public CoomiAutoBackupWorker(@NonNull Context context, @NonNull WorkerParameters parameters) {
        super(context, parameters);
    }

    @NonNull
    @Override
    public Result doWork() {
        Context context = getApplicationContext();
        SharedPreferences preferences = CoomiAutoBackup.preferences(context);
        if (!getInputData().getBoolean("force", false)
            && !preferences.getBoolean(CoomiAutoBackup.KEY_ENABLED, false)) return Result.success();
        Uri directoryUri = CoomiAutoBackup.directoryUri(context);
        if (directoryUri == null) return failure("未选择自动备份目录", true);
        if (!CoomiAutoBackup.OPERATION_LOCK.tryLock()) return Result.retry();

        File archive = null;
        DocumentFile partial = null;
        try {
            DocumentFile directory = DocumentFile.fromTreeUri(context, directoryUri);
            if (directory == null || !directory.isDirectory() || !directory.canWrite()) {
                return failure("备份目录授权已失效，请重新选择", true);
            }
            archive = CoomiBackupArchive.create(context);
            String stamp = new SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(new Date());
            String finalName = "coomi-backup-" + stamp + ".zip";
            partial = directory.createFile("application/octet-stream", finalName + ".partial");
            if (partial == null) throw new IllegalStateException("无法在目标目录创建临时文件");
            try (InputStream input = new FileInputStream(archive);
                 OutputStream output = context.getContentResolver().openOutputStream(partial.getUri(), "w")) {
                if (output == null) throw new IllegalStateException("无法写入目标目录");
                copy(input, output);
            }
            verifyDocument(context, partial.getUri());
            if (!partial.renameTo(finalName)) throw new IllegalStateException("目标目录不支持原子完成备份");
            partial = null;
            rotate(directory, preferences.getInt(CoomiAutoBackup.KEY_KEEP_COUNT, CoomiAutoBackup.DEFAULT_KEEP_COUNT));
            int interval = CoomiAutoBackup.normalizeInterval(
                preferences.getInt(CoomiAutoBackup.KEY_INTERVAL_HOURS, CoomiAutoBackup.DEFAULT_INTERVAL_HOURS));
            preferences.edit()
                .putLong(CoomiAutoBackup.KEY_LAST_SUCCESS, System.currentTimeMillis())
                .putLong(CoomiAutoBackup.KEY_LAST_SIZE, archive.length())
                .putString(CoomiAutoBackup.KEY_LAST_ERROR, "")
                .putLong(CoomiAutoBackup.KEY_NEXT_RUN,
                    System.currentTimeMillis() + TimeUnit.HOURS.toMillis(interval))
                .apply();
            return Result.success();
        } catch (SecurityException error) {
            return failure("备份目录授权已失效，请重新选择", true);
        } catch (Exception error) {
            return failure(safeMessage(error), false);
        } finally {
            if (partial != null) partial.delete();
            if (archive != null) archive.delete();
            CoomiAutoBackup.OPERATION_LOCK.unlock();
        }
    }

    private Result failure(String message, boolean disable) {
        Context context = getApplicationContext();
        if (disable) CoomiAutoBackup.disableForRevokedDirectory(context, message);
        else CoomiAutoBackup.preferences(context).edit()
            .putString(CoomiAutoBackup.KEY_LAST_ERROR, message).apply();
        return Result.failure();
    }

    private static void rotate(DocumentFile directory, int requestedKeep) {
        int keep = Math.max(1, Math.min(30, requestedKeep));
        List<DocumentFile> backups = new ArrayList<>();
        DocumentFile[] files = directory.listFiles();
        if (files == null) return;
        for (DocumentFile file : files) {
            String name = file.getName();
            if (file.isFile() && name != null && name.startsWith("coomi-backup-") && name.endsWith(".zip")) {
                backups.add(file);
            }
        }
        backups.sort(Comparator.comparingLong(DocumentFile::lastModified).reversed());
        for (int index = keep; index < backups.size(); index++) backups.get(index).delete();
    }

    private static void verifyDocument(Context context, Uri uri) throws Exception {
        int entries = 0;
        try (InputStream raw = context.getContentResolver().openInputStream(uri)) {
            if (raw == null) throw new IllegalStateException("无法重新打开写入后的备份包");
            try (ZipInputStream input = new ZipInputStream(raw)) {
                while (input.getNextEntry() != null) {
                    entries++;
                    byte[] buffer = new byte[8192];
                    while (input.read(buffer) != -1) { /* validate CRC and structure */ }
                    input.closeEntry();
                }
            }
        }
        if (entries == 0) throw new IllegalStateException("写入后的备份包校验失败");
    }

    private static void copy(InputStream input, OutputStream output) throws Exception {
        byte[] buffer = new byte[8192];
        int read;
        while ((read = input.read(buffer)) != -1) output.write(buffer, 0, read);
        output.flush();
    }

    private static String safeMessage(Exception error) {
        String message = error.getMessage();
        if (message == null || message.trim().isEmpty()) return error.getClass().getSimpleName();
        return message.length() > 240 ? message.substring(0, 240) : message;
    }
}
