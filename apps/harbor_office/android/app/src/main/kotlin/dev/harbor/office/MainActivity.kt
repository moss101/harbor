package dev.harbor.office

import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Harbor device root key, sealed under the hardware AndroidKeyStore.
 *
 * The NDK core (loaded via dlopen from Dart FFI) has no JavaVM and cannot
 * reach AndroidKeyStore directly, so this embedding is the platform
 * keystore adapter: a 32-byte device root is generated on first launch,
 * sealed with a non-exportable AES-256-GCM key that lives in the TEE /
 * StrongBox-backed AndroidKeyStore, and stored on disk ONLY in sealed
 * form. Each launch unwraps it here and hands the plaintext to the core
 * through `harbor_core_open_ex(device_root_hex)`; at rest nothing but the
 * Keystore-sealed blob ever exists.
 */
class MainActivity : FlutterActivity() {
    private val channelName = "dev.harbor.keystore"
    private val keystoreAlias = "harbor-device-root-wrap"
    private val blobFile = "harbor-device-root.bin"

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, channelName)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "getRootKey" -> {
                        try {
                            result.success(rootKeyHex())
                        } catch (e: Exception) {
                            result.error("keystore", e.message, null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
        // "Open in Harbor Office Suite": documents other apps hand us.
        // Content URIs are copied to a per-delivery folder in cacheDir
        // (off the UI thread); Dart imports the copy into the suite's own
        // store and deletes it. `getInitialPath` is the LAUNCH document
        // (consumed once); `onNewIntent` pushes `openPath` while running.
        val channel = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger, openChannelName
        )
        openChannel = channel
        channel.setMethodCallHandler { call, result ->
            when (call.method) {
                "getInitialPath" -> {
                    val launch = intent
                    if (launch == null || launch.getBooleanExtra(handledExtra, false) ||
                        launch.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0
                    ) {
                        result.success(null)
                    } else {
                        resolveAsync(launch) { result.success(it) }
                    }
                }
                else -> result.notImplemented()
            }
        }
        // System print dialog over a rendered PDF (office Print action):
        // PrintManager + an adapter that copies the finished PDF bytes
        // to the framework's destination. Nothing leaves the device
        // except the user's own print job.
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger, "dev.harbor.office/print"
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "printPdf" -> {
                    val data = call.argument<ByteArray>("data")
                    val name = call.argument<String>("name") ?: "document"
                    if (data == null) {
                        result.error("print", "missing data", null)
                        return@setMethodCallHandler
                    }
                    try {
                        printPdf(data, name)
                        result.success(true)
                    } catch (e: Exception) {
                        result.error("print", e.message, null)
                    }
                }
                else -> result.notImplemented()
            }
        }
        pruneIntakeCache()
    }

    /** Print ready PDF bytes through the system print framework. */
    private fun printPdf(bytes: ByteArray, jobName: String) {
        val manager = getSystemService(PRINT_SERVICE) as android.print.PrintManager
        val adapter = object : android.print.PrintDocumentAdapter() {
            override fun onLayout(
                oldAttributes: android.print.PrintAttributes?,
                newAttributes: android.print.PrintAttributes,
                cancellationSignal: android.os.CancellationSignal?,
                callback: LayoutResultCallback,
                extras: android.os.Bundle?
            ) {
                if (cancellationSignal?.isCanceled == true) {
                    callback.onLayoutCancelled()
                    return
                }
                val info = android.print.PrintDocumentInfo.Builder("print.pdf")
                    .setContentType(android.print.PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
                    .setPageCount(android.print.PrintDocumentInfo.PAGE_COUNT_UNKNOWN)
                    .build()
                callback.onLayoutFinished(info, true)
            }

            override fun onWrite(
                pages: Array<out android.print.PageRange>?,
                destination: android.os.ParcelFileDescriptor,
                cancellationSignal: android.os.CancellationSignal?,
                callback: WriteResultCallback
            ) {
                try {
                    java.io.FileOutputStream(destination.fileDescriptor).use { out ->
                        out.write(bytes)
                    }
                    callback.onWriteFinished(arrayOf(android.print.PageRange.ALL_PAGES))
                } catch (e: Exception) {
                    callback.onWriteFailed(e.message)
                }
            }
        }
        manager.print(jobName, adapter, android.print.PrintAttributes.Builder().build())
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        resolveAsync(intent) { payload ->
            if (payload != null) openChannel?.invokeMethod("openPath", payload)
        }
    }

    private val openChannelName = "dev.harbor.office/open"
    private val handledExtra = "dev.harbor.office.HANDLED"
    private val maxOpenBytes = 256L * 1024 * 1024
    private var openChannel: MethodChannel? = null

    /**
     * Resolve a VIEW intent to `{path, name}` / `{error, name}` on a worker
     * thread and deliver it on the UI thread. null when the intent carries
     * no document. The intent is marked handled so a recreated activity
     * never re-opens the same document.
     */
    private fun resolveAsync(i: Intent, deliver: (Map<String, String>?) -> Unit) {
        val uri = i.data
        if (i.action != Intent.ACTION_VIEW || uri == null) {
            deliver(null)
            return
        }
        i.putExtra(handledExtra, true)
        Thread {
            val payload = resolveDocument(uri)
            runOnUiThread { deliver(payload) }
        }.start()
    }

    private fun resolveDocument(uri: Uri): Map<String, String> {
        val mime = contentResolver.getType(uri)
        val display = displayName(uri)
        val name = safeName(display, mime)
        if (uri.scheme == "file") {
            // Direct file paths only work for files we can already read.
            val f = File(uri.path ?: return mapOf("error" to "unreadable", "name" to name))
            return if (f.canRead()) mapOf("path" to f.absolutePath, "name" to name)
            else mapOf("error" to "unreadable", "name" to name)
        }
        return try {
            val dir = File(File(cacheDir, "open-intake"), System.nanoTime().toString())
            dir.mkdirs()
            val out = File(dir, name)
            var copied = 0L
            val input = contentResolver.openInputStream(uri)
                ?: return mapOf("error" to "unreadable", "name" to name)
            input.use { src ->
                out.outputStream().use { dst ->
                    val buf = ByteArray(64 * 1024)
                    while (true) {
                        val n = src.read(buf)
                        if (n < 0) break
                        copied += n
                        if (copied > maxOpenBytes) {
                            dst.close()
                            out.delete()
                            dir.delete()
                            return mapOf("error" to "too_large", "name" to name)
                        }
                        dst.write(buf, 0, n)
                    }
                }
            }
            mapOf("path" to out.absolutePath, "name" to name)
        } catch (e: Exception) {
            mapOf("error" to "unreadable", "name" to name)
        }
    }

    private fun displayName(uri: Uri): String? = try {
        if (uri.scheme == "content") {
            contentResolver.query(uri, null, null, null, null)?.use { c ->
                val idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (idx >= 0 && c.moveToFirst()) c.getString(idx) else null
            }
        } else uri.lastPathSegment
    } catch (e: Exception) {
        null
    }

    /**
     * Basename only, no separators/control characters/leading dots, and an
     * extension recovered from the MIME type when the provider's display
     * name has none (several providers report "document" + a MIME type).
     */
    private fun safeName(raw: String?, mime: String?): String {
        var name = (raw ?: "document").substringAfterLast('/').substringAfterLast('\\')
            .replace(Regex("[\\u0000-\\u001f<>:\"|?*]"), "_")
            .trimStart('.')
            .trim()
        if (name.isEmpty()) name = "document"
        if (!name.contains('.')) {
            val ext = when (mime) {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document" -> "docx"
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" -> "xlsx"
                "application/vnd.openxmlformats-officedocument.presentationml.presentation" -> "pptx"
                "application/pdf" -> "pdf"
                else -> null
            }
            if (ext != null) name = "$name.$ext"
        }
        return if (name.length > 120) name.take(120) else name
    }

    /** Copies Dart never imported (crash mid-delivery) must not accumulate. */
    private fun pruneIntakeCache() {
        val root = File(cacheDir, "open-intake")
        val cutoff = System.currentTimeMillis() - 24L * 60 * 60 * 1000
        root.listFiles()?.forEach { if (it.lastModified() < cutoff) it.deleteRecursively() }
    }

    private fun wrapKey(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(keystoreAlias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore"
        )
        generator.init(
            KeyGenParameterSpec.Builder(
                keystoreAlias,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return generator.generateKey()
    }

    /** nonce(12) || GCM ciphertext, sealed under the non-exportable key. */
    private fun blob(): File = File(filesDir, blobFile)

    private fun rootKeyHex(): String {
        val key = wrapKey()
        val existing = blob()
        val root = if (existing.exists()) {
            val sealed = existing.readBytes()
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, sealed, 0, 12))
            cipher.doFinal(sealed, 12, sealed.size - 12)
        } else {
            val fresh = ByteArray(32).also { SecureRandom().nextBytes(it) }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, key)
            val sealed = cipher.doFinal(fresh)
            val nonce = cipher.iv
            existing.writeBytes(nonce + sealed)
            fresh
        }
        return root.joinToString("") { "%02x".format(it) }
    }
}
