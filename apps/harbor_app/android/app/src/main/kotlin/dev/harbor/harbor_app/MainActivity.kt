package dev.harbor.harbor_app

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
