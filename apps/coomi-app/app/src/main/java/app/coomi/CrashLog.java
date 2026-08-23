package app.coomi;

import android.content.Context;
import android.os.Build;
import android.util.Log;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.FileWriter;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;

/**
 * 崩溃采集：Java 未捕获异常 → crash.log（含线程栈与设备信息）；
 * 启动时与崩溃时各快照一次 logcat 尾部 → logcat_boot.log / logcat_crash.log，
 * 用于定位不触发 Java 回调的原生崩溃（如老机/鸿蒙上的打开即闪退）。
 */
public final class CrashLog {

    private static final String TAG = "CrashLog";
    private static final int MAX_LOGCAT_BYTES = 1024 * 1024;
    private static volatile boolean sInstalled = false;

    private CrashLog() {
    }

    public static synchronized void install(Context context) {
        if (sInstalled) return;
        sInstalled = true;
        final Context appContext = context.getApplicationContext();
        dumpLogcat(appContext, "logcat_boot.log");
        Thread.UncaughtExceptionHandler previous = Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler((thread, throwable) -> {
            try {
                writeCrash(appContext, thread, throwable);
                dumpLogcat(appContext, "logcat_crash.log");
            } catch (Throwable ignored) {
                // 采集中不允许再次抛出
            }
            if (previous != null) {
                previous.uncaughtException(thread, throwable);
            }
        });
        Log.i(TAG, "CrashLog installed");
    }

    private static void writeCrash(Context context, Thread thread, Throwable throwable) {
        try {
            StringWriter stack = new StringWriter();
            throwable.printStackTrace(new PrintWriter(stack));
            StringBuilder builder = new StringBuilder();
            builder.append("==== Coomi Crash ").append(stamp()).append(" ====\n");
            builder.append("thread: ").append(thread.getName())
                    .append(" (id=").append(thread.getId()).append(")\n");
            builder.append("device: ").append(Build.MANUFACTURER).append(' ')
                    .append(Build.MODEL).append(" / Android ")
                    .append(Build.VERSION.RELEASE)
                    .append(" (sdk ").append(Build.VERSION.SDK_INT)
                    .append(", abi ").append(Build.CPU_ABI).append(")\n");
            builder.append("build: ").append(Build.VERSION.RELEASE).append('\n');
            builder.append(stack);
            builder.append('\n');
            File log = new File(logsDir(context), "crash.log");
            FileWriter writer = new FileWriter(log, true);
            writer.write(builder.toString());
            writer.close();
            Log.e(TAG, "crash written to " + log.getAbsolutePath());
        } catch (Throwable ignored) {
        }
    }

    private static void dumpLogcat(Context context, String name) {
        try {
            Process process = Runtime.getRuntime().exec(
                    new String[]{"logcat", "-d", "-t", "400", "-v", "threadtime"});
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(process.getInputStream()), 8192);
                 FileOutputStream output = new FileOutputStream(
                         new File(logsDir(context), name))) {
                String line;
                int total = 0;
                StringBuilder buffer = new StringBuilder(8192);
                while ((line = reader.readLine()) != null) {
                    buffer.append(line).append('\n');
                    total += line.length() + 1;
                    if (total >= MAX_LOGCAT_BYTES) break;
                }
                byte[] bytes = buffer.toString().getBytes("UTF-8");
                output.write(bytes);
                output.flush();
            }
        } catch (Throwable ignored) {
            // 部分 ROM 禁止应用读取 logcat，忽略即可
        }
    }

    private static File logsDir(Context context) {
        File dir = new File(context.getApplicationContext().getFilesDir(), "logs");
        if (!dir.isDirectory() && !dir.mkdirs()) {
            return context.getApplicationContext().getFilesDir();
        }
        return dir;
    }

    private static String stamp() {
        return new SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US)
                .format(new Date());
    }
}
