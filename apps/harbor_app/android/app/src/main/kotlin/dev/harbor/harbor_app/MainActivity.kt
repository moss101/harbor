package dev.harbor.harbor_app

import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.ByteArrayOutputStream
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
        // Video -> sampled JPEG frames for multimodal indexing (the Rust core
        // carries no video decoder). Dart: lib/services/video_frames.dart.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.harbor.video_frames")
            .setMethodCallHandler { call, result ->
                val path = call.argument<String>("path")
                if (call.method != "sample" || path == null) {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                val maxFrames = call.argument<Int>("maxFrames") ?: 24
                Thread {
                    try {
                        val frames = sampleVideoFrames(path, maxFrames, 512)
                        runOnUiThread { result.success(frames) }
                    } catch (e: Exception) {
                        runOnUiThread { result.error("video", e.message, null) }
                    }
                }.start()
            }
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
    }

    /**
     * Up to [maxFrames] JPEG frames evenly spaced across the video (about
     * one per second, the rate the embedding model expects). Exact seeking
     * (OPTION_CLOSEST): sync-frame seeking returns the same frame repeatedly
     * for short clips.
     */
    private fun sampleVideoFrames(path: String, maxFrames: Int, maxEdge: Int): List<ByteArray> {
        val retriever = MediaMetadataRetriever()
        try {
            retriever.setDataSource(path)
            val durationMs = retriever
                .extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)
                ?.toLongOrNull()
                ?: throw IllegalStateException("unreadable video")
            if (durationMs <= 0) throw IllegalStateException("unreadable video")
            val count = maxOf(1, minOf(maxFrames, Math.ceil(durationMs / 1000.0).toInt()))
            val frames = ArrayList<ByteArray>(count)
            for (i in 0 until count) {
                val atUs = (durationMs * 1000.0 * (i + 0.5) / count).toLong()
                var bitmap = retriever.getFrameAtTime(atUs, MediaMetadataRetriever.OPTION_CLOSEST)
                    ?: throw IllegalStateException("no frame at ${atUs}us")
                val longest = maxOf(bitmap.width, bitmap.height)
                if (longest > maxEdge) {
                    val scale = maxEdge.toFloat() / longest
                    bitmap = Bitmap.createScaledBitmap(
                        bitmap,
                        maxOf(1, (bitmap.width * scale).toInt()),
                        maxOf(1, (bitmap.height * scale).toInt()),
                        true
                    )
                }
                val out = ByteArrayOutputStream()
                bitmap.compress(Bitmap.CompressFormat.JPEG, 80, out)
                frames.add(out.toByteArray())
            }
            return frames
        } finally {
            retriever.release()
        }
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
