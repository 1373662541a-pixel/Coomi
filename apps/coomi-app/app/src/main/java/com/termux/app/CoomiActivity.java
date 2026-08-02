package com.termux.app;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.text.TextUtils;
import android.view.View;
import android.webkit.JavascriptInterface;
import android.content.res.Configuration;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.TextView;

import androidx.core.content.ContextCompat;

import app.coomi.CoomiConstants;
import app.coomi.CoomiDemo;
import app.coomi.CoomiEngineMonitor;
import app.coomi.CoomiService;
import app.coomi.CoomiDashboardActivity;
import com.termux.R;
import com.termux.shared.logger.Logger;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.FileWriter;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Coomi chat screen — hosts the Vue frontend served by coomi-rs.
 *
 * The heavy lifting lives in {@link CoomiService}: it deploys the native executable,
 * starts {@code coomi serve} and reports the port it bound to. This activity
 * only waits for the engine to answer its health endpoint, then points the WebView at it.
 */
public class CoomiActivity extends Activity {

    private static final String LOG_TAG = "CoomiActivity";
    private static final int REQUEST_IMPORT_FILES = 2101;
    private static final int REQUEST_AUTHORIZE_TREE = 2102;
    private static final int REQUEST_EXPORT_FILE = 2103;

    /** Intent extra：直达前端 hash 路由，如 "#/catalog"。 */
    public static final String EXTRA_ROUTE = "coomi.route";

    private WebView mWebView;
    private View mSplash;
    private View mSplashSpinner;
    private TextView mLoadingText;
    private TextView mLoadingDetail;
    private Button mRetryButton;

    private final Handler mHandler = new Handler(Looper.getMainLooper());
    private CoomiService mCoomiService;
    private boolean mBound;
    private boolean mStartRequested;
    private boolean mPageLoaded;
    private int mAutomaticRecoveryAttempts;
    private String mPendingExportPath;
    private String mPendingExportName;
    private String mPendingImportRequestId;
    private String mPendingExportRequestId;

    private final ServiceConnection mConnection = new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            mCoomiService = ((CoomiService.LocalBinder) service).getService();
            mBound = true;
            ensureEngineRunning();
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            mCoomiService = null;
            mBound = false;
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_coomi);
        mWebView = findViewById(R.id.coomi_webview);
        mSplash = findViewById(R.id.coomi_splash);
        mSplashSpinner = findViewById(R.id.coomi_splash_spinner);
        mLoadingText = findViewById(R.id.coomi_loading_text);
        mLoadingDetail = findViewById(R.id.coomi_loading_detail);
        mRetryButton = findViewById(R.id.btn_coomi_retry);
        mRetryButton.setOnClickListener(v -> retryStart());
        configureWebView();

        showLoading(getString(R.string.coomi_starting));

        // 演示包不启动引擎，直接使用 APK 里的前端。
        if (CoomiDemo.isEnabled()) {
            startDemo();
            return;
        }

        // Keep the engine alive while the user is away from the app.
        startService(new Intent(this, CoomiEngineMonitor.class));

        Intent intent = new Intent(this, CoomiService.class);
        startService(intent);
        bindService(intent, mConnection, Context.BIND_AUTO_CREATE);
    }

    /**
     * 演示模式的「启动」：把 web.zip 解到 filesDir/web，然后加载 https://coomi.local/。
     * 请求全部由 {@link CoomiDemo#serve} 就地应答，不出网、不碰引擎。
     */
    private void startDemo() {
        showLoading(getString(R.string.coomi_demo_loading));
        new Thread(() -> {
            final File dir = CoomiDemo.ensureWebDir(this);
            runOnUiThread(() -> {
                if (mWebView == null) return;
                if (dir == null) {
                    showFailure(getString(R.string.coomi_demo_failed), null);
                    return;
                }
                mPageLoaded = true;
                mWebView.loadUrl(CoomiDemo.START_URL);
            });
        }).start();
    }

    /** Start the engine unless it is already up, then wait for health. */
    private void ensureEngineRunning() {
        if (mStartRequested || mCoomiService == null) return;
        mStartRequested = true;

        mCoomiService.getEngineStatus(status -> {
            if ("running".equals(status.stdout)) {
                onEngineReady(mCoomiService.getEnginePort());
                return;
            }
            showLoading(getString(R.string.coomi_engine_starting));
            mCoomiService.startEngine(result -> {
                if (!result.success) {
                    attemptAutomaticRecovery();
                    return;
                }
                waitForEngine();
            });
        });
    }

    private void attemptAutomaticRecovery() {
        if (mCoomiService != null && mAutomaticRecoveryAttempts < 1) {
            mAutomaticRecoveryAttempts++;
            showLoading(getString(R.string.coomi_engine_starting));
            mCoomiService.restartEngine(result -> {
                if (result.success) waitForEngine();
                else showFailure(getString(R.string.coomi_engine_exited), null);
            });
            return;
        }
        showFailure(getString(R.string.coomi_engine_exited), null);
    }

    /** 失败后允许原地重试，否则用户只能杀进程。 */
    private void retryStart() {
        mStartRequested = false;
        runOnUiThread(() -> {
            mRetryButton.setVisibility(View.GONE);
            mLoadingDetail.setVisibility(View.GONE);
            mSplashSpinner.setVisibility(View.VISIBLE);
        });
        if (CoomiDemo.isEnabled()) {
            startDemo();
            return;
        }
        showLoading(getString(R.string.coomi_engine_starting));
        ensureEngineRunning();
    }

    /** Poll the service until the bridge answers, surfacing log tails as progress. */
    private void waitForEngine() {
        final long deadline = System.currentTimeMillis()
            + CoomiConstants.ENGINE_START_TIMEOUT_SEC * 1000L;

        Runnable poll = new Runnable() {
            @Override
            public void run() {
                if (mCoomiService == null) return;
                mCoomiService.getEngineStatus(status -> {
                    if ("running".equals(status.stdout)) {
                        onEngineReady(mCoomiService.getEnginePort());
                        return;
                    }
                    if ("stopped".equals(status.stdout)) {
                        attemptAutomaticRecovery();
                        return;
                    }
                    if (System.currentTimeMillis() > deadline) {
                        attemptAutomaticRecovery();
                        return;
                    }
                    mHandler.postDelayed(this, 2000);
                });
            }
        };
        mHandler.postDelayed(poll, 1000);
    }

    private void onEngineReady(int port) {
        if (mPageLoaded) return;
        mPageLoaded = true;
        Logger.logInfo(LOG_TAG, "Engine ready on port " + port);
        // 支持从控制台直达特定前端路由（如 SKILL/MCP 管理页 #/catalog）。
        String route = getIntent().getStringExtra(EXTRA_ROUTE);
        String url = "http://127.0.0.1:" + port + "/"
            + (route != null && route.startsWith("#") ? route : "");
        final String target = url;
        runOnUiThread(() -> mWebView.loadUrl(target));
    }

    private void configureWebView() {
        WebSettings s = mWebView.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        // The bridge serves everything over loopback HTTP; no local file access needed.
        s.setAllowContentAccess(false);
        s.setAllowFileAccess(false);
        WebView.setWebContentsDebuggingEnabled(true);
        mWebView.addJavascriptInterface(new AndroidBridge(), "CoomiAndroid");

        mWebView.setWebViewClient(new WebViewClient() {
            /** 演示包用假域名装本地文件；正式包不拦，让它照常走 loopback。 */
            @Override
            public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                if (!CoomiDemo.isEnabled()) return null;
                return CoomiDemo.serve(CoomiActivity.this, request.getUrl());
            }

            @Override
            public void onPageFinished(WebView view, String url) {
                // 前端已经可见了，整块闪屏一起收掉，避免残留的 spinner 盖在页面上。
                mSplash.setVisibility(View.GONE);
                mWebView.setVisibility(View.VISIBLE);
                // 页面加载完把系统深浅色同步给前端（重新加载会清掉之前注入的属性）。
                applyThemeToWebView();
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                if ("coomi".equals(request.getUrl().getScheme())
                    && "dashboard".equals(request.getUrl().getHost())) {
                    openDashboard();
                    return true;
                }
                return false;
            }
        });
    }

    /** 主状态行：一行短文案，顺手清掉上一次失败留下的日志和重试按钮。 */
    private void showLoading(String text) {
        runOnUiThread(() -> {
            if (mLoadingText == null) return;
            mLoadingText.setTextColor(
                ContextCompat.getColor(mLoadingText.getContext(), R.color.coomi_text_2));
            mLoadingText.setText(text);
            mLoadingDetail.setVisibility(View.GONE);
            mRetryButton.setVisibility(View.GONE);
            mSplashSpinner.setVisibility(View.VISIBLE);
        });
    }

    /** 副状态行：等引擎的时候把日志尾巴显出来，让等待有内容可看。 */
    private void showDetail(String detail) {
        runOnUiThread(() -> {
            if (mLoadingDetail == null) return;
            if (TextUtils.isEmpty(detail)) {
                mLoadingDetail.setVisibility(View.GONE);
                return;
            }
            mLoadingDetail.setText(detail.trim());
            mLoadingDetail.setVisibility(View.VISIBLE);
        });
    }

    /** 失败终态：只显示可操作的用户文案，诊断信息留在日志中。 */
    private void showFailure(String message, String detail) {
        runOnUiThread(() -> {
            if (mLoadingText == null) return;
            mLoadingText.setTextColor(
                ContextCompat.getColor(mLoadingText.getContext(), R.color.coomi_danger));
            mLoadingText.setText(message);
            mSplashSpinner.setVisibility(View.GONE);
            mRetryButton.setVisibility(View.VISIBLE);
            mLoadingDetail.setVisibility(View.GONE);
        });
    }

    private void openDashboard() {
        Intent intent = new Intent(this, CoomiDashboardActivity.class);
        intent.addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT);
        startActivity(intent);
        finish();
    }

    /** 系统是否处于深色模式（Theme.Coomi 刻意固定浅色，但 Web 内容需跟随系统）。 */
    private boolean isSystemDark() {
        int mode = getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK;
        return mode == Configuration.UI_MODE_NIGHT_YES;
    }

    /** 把系统深浅色写入 <html data-theme>，前端 global.css 据此切换暗色主题。 */
    private void applyThemeToWebView() {
        if (mWebView == null) return;
        runOnUiThread(() -> evaluateJavascript(
            "document.documentElement.setAttribute('data-theme','" + (isSystemDark() ? "dark" : "light") + "')",
            null));
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        // 系统切换深浅色时实时同步到 Web 内容（configChanges 含 uiMode，Activity 不重建）。
        applyThemeToWebView();
    }

    private final class AndroidBridge {
        @JavascriptInterface
        public void openDashboard() { runOnUiThread(CoomiActivity.this::openDashboard); }

        @JavascriptInterface
        public void importFiles() {
            mPendingImportRequestId = null;
            runOnUiThread(CoomiActivity.this::launchImportPicker);
        }

        @JavascriptInterface
        public void importFilesForRequest(String requestId) {
            mPendingImportRequestId = requestId;
            runOnUiThread(CoomiActivity.this::launchImportPicker);
        }

        @JavascriptInterface
        public void authorizeFolder() {
            runOnUiThread(() -> {
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                    | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
                startActivityForResult(intent, REQUEST_AUTHORIZE_TREE);
            });
        }

        @JavascriptInterface
        public void exportFile(String path, String suggestedName) {
            mPendingExportRequestId = null;
            launchExportPicker(path, suggestedName);
        }

        /** 用系统其它 app 打开文件（图片/文档等），走 FileProvider 授权。 */
        @JavascriptInterface
        public void openFile(String path) {
            runOnUiThread(() -> {
                try {
                    File file = new File(path);
                    if (!file.isFile()) {
                        Toast.makeText(CoomiActivity.this, "文件不存在：" + path, Toast.LENGTH_SHORT).show();
                        return;
                    }
                    android.net.Uri uri = androidx.core.content.FileProvider.getUriForFile(
                        CoomiActivity.this, getPackageName() + ".fileprovider", file);
                    Intent intent = new Intent(Intent.ACTION_VIEW);
                    intent.setDataAndType(uri, mimeFromName(file.getName()));
                    intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                    startActivity(intent);
                } catch (Exception error) {
                    Toast.makeText(CoomiActivity.this,
                        "无法打开文件：" + error.getMessage(), Toast.LENGTH_SHORT).show();
                }
            });
        }

        private String mimeFromName(String name) {
            String ext = name.contains(".") ? name.substring(name.lastIndexOf('.') + 1).toLowerCase() : "";
            String mime = android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);
            if (mime != null) return mime;
            switch (ext) {
                case "md": case "markdown": case "txt": case "log": case "sh":
                case "py": case "rs": case "js": case "ts": case "vue": case "json":
                case "toml": case "yaml": case "yml": case "conf": case "ini":
                    return "text/plain";
                case "svg": return "image/svg+xml";
                default: return "application/octet-stream";
            }
        }

        @JavascriptInterface
        public void exportFileForRequest(String requestId, String path, String suggestedName) {
            mPendingExportRequestId = requestId;
            launchExportPicker(path, suggestedName);
        }

        private void launchExportPicker(String path, String suggestedName) {
            runOnUiThread(() -> {
                File source = new File(path);
                if (!source.isFile()) {
                    emitTransferProgress("导出失败：文件不存在", 0);
                    return;
                }
                mPendingExportPath = source.getAbsolutePath();
                mPendingExportName = TextUtils.isEmpty(suggestedName) ? source.getName() : suggestedName;
                Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType("application/octet-stream");
                intent.putExtra(Intent.EXTRA_TITLE, mPendingExportName);
                startActivityForResult(intent, REQUEST_EXPORT_FILE);
            });
        }
    }

    private void launchImportPicker() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        startActivityForResult(intent, REQUEST_IMPORT_FILES);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (resultCode != RESULT_OK || data == null) {
            if (requestCode == REQUEST_IMPORT_FILES && mPendingImportRequestId != null) {
                emitFilesImported(new JSONArray(), mPendingImportRequestId);
                mPendingImportRequestId = null;
            } else if (requestCode == REQUEST_EXPORT_FILE && mPendingExportRequestId != null) {
                emitFileExported(mPendingExportRequestId, null);
                mPendingExportRequestId = null;
            }
            return;
        }
        if (requestCode == REQUEST_IMPORT_FILES) {
            List<Uri> uris = new ArrayList<>();
            if (data.getClipData() != null) {
                for (int i = 0; i < data.getClipData().getItemCount(); i++) {
                    uris.add(data.getClipData().getItemAt(i).getUri());
                }
            } else if (data.getData() != null) {
                uris.add(data.getData());
            }
            new Thread(() -> importUris(uris), "coomi-file-import").start();
        } else if (requestCode == REQUEST_AUTHORIZE_TREE && data.getData() != null) {
            authorizeTree(data.getData(), data.getFlags());
        } else if (requestCode == REQUEST_EXPORT_FILE && data.getData() != null) {
            Uri target = data.getData();
            new Thread(() -> exportToUri(target), "coomi-file-export").start();
        }
    }

    private void importUris(List<Uri> uris) {
        File inbox = new File(CoomiConstants.COOMI_INBOX);
        if (!inbox.isDirectory() && !inbox.mkdirs()) {
            emitTransferProgress("无法创建 Agent inbox", 0);
            return;
        }
        JSONArray paths = new JSONArray();
        for (int index = 0; index < uris.size(); index++) {
            Uri uri = uris.get(index);
            String name = queryDisplayName(uri);
            File destination = uniqueDestination(inbox, name);
            emitTransferProgress("正在导入 " + name, (index * 100) / Math.max(uris.size(), 1));
            try (InputStream input = getContentResolver().openInputStream(uri);
                 OutputStream output = new FileOutputStream(destination)) {
                if (input == null) throw new IllegalStateException("无法读取所选文件");
                copyStream(input, output);
                rememberOrigin(destination, uri.toString(), name);
                paths.put(destination.getAbsolutePath());
            } catch (Exception error) {
                Logger.logError(LOG_TAG, "File import failed: " + error.getMessage());
                emitTransferProgress("导入失败：" + name, 0);
            }
        }
        emitFilesImported(paths, mPendingImportRequestId);
        mPendingImportRequestId = null;
    }

    private void authorizeTree(Uri uri, int flags) {
        try {
            int persistFlags = flags & (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            getContentResolver().takePersistableUriPermission(uri, persistFlags);
            String path = treeUriToPath(uri);
            File inbox = new File(CoomiConstants.COOMI_INBOX);
            if (!inbox.isDirectory()) inbox.mkdirs();
            rememberOrigin(new File(path), uri.toString(), "authorized-tree");
            JSONArray paths = new JSONArray();
            paths.put(path);
            emitFilesImported(paths, null);
        } catch (Exception error) {
            Logger.logError(LOG_TAG, "Folder authorization failed: " + error.getMessage());
            emitTransferProgress("目录授权失败", 0);
        }
    }

    private String treeUriToPath(Uri uri) {
        String documentId = DocumentsContract.getTreeDocumentId(uri);
        String[] parts = documentId.split(":", 2);
        String relative = parts.length > 1 ? parts[1] : "";
        String root = parts[0].equalsIgnoreCase("primary") ? "/storage/emulated/0" : "/storage/" + parts[0];
        return relative.isEmpty() ? root : root + "/" + relative;
    }

    private void exportToUri(Uri target) {
        File source = new File(mPendingExportPath == null ? "" : mPendingExportPath);
        try (InputStream input = new FileInputStream(source);
             OutputStream output = getContentResolver().openOutputStream(target, "w")) {
            if (output == null) throw new IllegalStateException("无法写入目标文件");
            emitTransferProgress("正在导出 " + source.getName(), 10);
            copyStream(input, output);
            emitTransferProgress("文件已导出", 100);
            emitFileExported(mPendingExportRequestId, source.getAbsolutePath());
        } catch (Exception error) {
            Logger.logError(LOG_TAG, "File export failed: " + error.getMessage());
            emitTransferProgress("导出失败：" + error.getMessage(), 0);
            emitFileExported(mPendingExportRequestId, null);
        } finally {
            mPendingExportPath = null;
            mPendingExportName = null;
            mPendingExportRequestId = null;
        }
    }

    private static void copyStream(InputStream input, OutputStream output) throws Exception {
        byte[] buffer = new byte[128 * 1024];
        int count;
        while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
        output.flush();
    }

    private String queryDisplayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                String name = cursor.getString(0);
                if (!TextUtils.isEmpty(name)) return sanitizeName(name);
            }
        } catch (Exception ignored) {}
        return "file-" + System.currentTimeMillis();
    }

    private static String sanitizeName(String name) {
        String safe = name.replaceAll("[\\\\/:*?\"<>|]", "_").trim();
        return safe.isEmpty() ? "file" : safe;
    }

    private static File uniqueDestination(File directory, String name) {
        File candidate = new File(directory, name);
        if (!candidate.exists()) return candidate;
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String extension = dot > 0 ? name.substring(dot) : "";
        int suffix = 2;
        while (candidate.exists()) candidate = new File(directory, stem + "-" + suffix++ + extension);
        return candidate;
    }

    private void rememberOrigin(File local, String uri, String displayName) {
        File index = new File(CoomiConstants.COOMI_INBOX, ".origins.jsonl");
        try (FileWriter writer = new FileWriter(index, true)) {
            JSONObject entry = new JSONObject();
            entry.put("localPath", local.getAbsolutePath());
            entry.put("originalUri", uri);
            entry.put("originalName", displayName);
            entry.put("recordedAt", System.currentTimeMillis());
            writer.write(entry.toString());
            writer.write("\n");
        } catch (Exception error) {
            Logger.logError(LOG_TAG, "Cannot record file origin: " + error.getMessage());
        }
    }

    private void emitTransferProgress(String message, int progress) {
        runOnUiThread(() -> evaluateJavascript("window.dispatchEvent(new CustomEvent('coomi:file-transfer-progress',{detail:{message:"
            + JSONObject.quote(message) + ",progress:" + progress + "}}))"));
    }

    private void emitFilesImported(JSONArray paths, String requestId) {
        String request = requestId == null ? "null" : JSONObject.quote(requestId);
        runOnUiThread(() -> evaluateJavascript("window.dispatchEvent(new CustomEvent('coomi:files-imported',{detail:{paths:"
            + paths.toString() + ",requestId:" + request + "}}))"));
    }

    private void emitFileExported(String requestId, String path) {
        if (requestId == null) return;
        String exportedPath = path == null ? "null" : JSONObject.quote(path);
        runOnUiThread(() -> evaluateJavascript("window.dispatchEvent(new CustomEvent('coomi:file-exported',{detail:{requestId:"
            + JSONObject.quote(requestId) + ",path:" + exportedPath + "}}))"));
    }

    private void evaluateJavascript(String script) {
        if (mWebView != null) mWebView.evaluateJavascript(script, null);
    }

    @Override
    public void onBackPressed() {
        // 需求：对话界面返回 → 控制台界面；控制台再返回才是退出 app。
        openDashboard();
    }

    @Override
    protected void onDestroy() {
        mHandler.removeCallbacksAndMessages(null);
        if (mBound) {
            unbindService(mConnection);
            mBound = false;
        }
        // The engine keeps running under CoomiEngineMonitor; only drop the view.
        if (mWebView != null) {
            mWebView.destroy();
            mWebView = null;
        }
        super.onDestroy();
    }
}
