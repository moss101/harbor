package dev.harbor.harbor_app

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Build
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.os.Bundle
import androidx.core.content.FileProvider
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.io.RandomAccessFile
import java.util.UUID

/**
 * Live capture for the composer (channel `dev.harbor/capture`; Dart:
 * lib/services/capture.dart). Everything stays on the device: recordings and
 * photos are temp files the Dart side deletes after reading, and speech is the
 * OS's ON-DEVICE recognizer only (never the network recognizer).
 *
 * Recording is 16 kHz mono PCM16 in a WAV, exactly the format the on-device
 * recognizer takes as an audio source, so a recording can be transcribed
 * without any decoder. Other audio files are not transcribed here.
 */
class HarborCapture(private val activity: Activity) {
    private val sampleRate = 16_000
    private val reqMic = 7101
    private val reqPhoto = 7102

    private var recorder: AudioRecord? = null
    private var recordThread: Thread? = null
    @Volatile private var recording = false
    private var recordFile: File? = null

    private var pendingPermission: (() -> Unit)? = null
    private var pendingDenied: (() -> Unit)? = null
    private var photoResult: MethodChannel.Result? = null
    private var photoFile: File? = null
    private var recognizer: SpeechRecognizer? = null

    private fun onDeviceSpeech() =
        Build.VERSION.SDK_INT >= 33 && SpeechRecognizer.isOnDeviceRecognitionAvailable(activity)

    fun handle(method: String, args: Map<*, *>?, result: MethodChannel.Result) {
        when (method) {
            "support" -> result.success(
                mapOf(
                    "record" to activity.packageManager.hasSystemFeature(PackageManager.FEATURE_MICROPHONE),
                    "photo" to (Intent(MediaStore.ACTION_IMAGE_CAPTURE)
                        .resolveActivity(activity.packageManager) != null),
                    "transcribe" to onDeviceSpeech(),
                )
            )
            "recordStart" -> recordStart(result)
            "recordStop" -> recordStop(result)
            "recordCancel" -> { recordCancel(); result.success(null) }
            "takePhoto" -> takePhoto(result)
            "transcribe" -> transcribe(args?.get("path") as? String, args?.get("locale") as? String, result)
            "transcribeCancel" -> { destroyRecognizer(); result.success(null) }
            else -> result.notImplemented()
        }
    }

    // --- permissions / activity results (forwarded from MainActivity) -------

    fun onRequestPermissionsResult(code: Int, granted: Boolean) {
        if (code != reqMic) return
        val ok = pendingPermission
        val no = pendingDenied
        pendingPermission = null
        pendingDenied = null
        if (granted) ok?.invoke() else no?.invoke()
    }

    fun onActivityResult(code: Int, resultCode: Int) {
        if (code != reqPhoto) return
        val result = photoResult ?: return
        val file = photoFile
        photoResult = null
        photoFile = null
        if (resultCode == Activity.RESULT_OK && file != null && file.length() > 0) {
            result.success(file.absolutePath)
        } else {
            file?.delete()
            result.success(null) // cancelled: Dart sees null, not an error
        }
    }

    // --- microphone ---------------------------------------------------------

    private fun recordStart(result: MethodChannel.Result) {
        recordCancel()
        val begin = {
            try {
                val min = AudioRecord.getMinBufferSize(
                    sampleRate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT
                )
                val rec = AudioRecord(
                    MediaRecorder.AudioSource.MIC, sampleRate, AudioFormat.CHANNEL_IN_MONO,
                    AudioFormat.ENCODING_PCM_16BIT, maxOf(min, sampleRate)
                )
                if (rec.state != AudioRecord.STATE_INITIALIZED) {
                    result.error("record", "the microphone could not be opened", null)
                } else {
                    val file = File(activity.cacheDir, "harbor-dictation-${UUID.randomUUID()}.wav")
                    RandomAccessFile(file, "rw").use { it.write(ByteArray(44)) } // header, patched at stop
                    recorder = rec
                    recordFile = file
                    recording = true
                    rec.startRecording()
                    recordThread = Thread {
                        val buf = ByteArray(4096)
                        RandomAccessFile(file, "rw").use { out ->
                            out.seek(44)
                            while (recording) {
                                val n = rec.read(buf, 0, buf.size)
                                if (n > 0) out.write(buf, 0, n)
                            }
                        }
                    }.also { it.start() }
                    result.success(null)
                }
            } catch (e: Exception) {
                result.error("record", e.message, null)
            }
        }
        if (activity.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            begin()
        } else {
            pendingPermission = begin
            pendingDenied = { result.error("permission", "microphone access denied", null) }
            activity.requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), reqMic)
        }
    }

    private fun stopRecorder(): File? {
        val rec = recorder ?: return null
        val file = recordFile
        recording = false
        recordThread?.join(2_000)
        try { rec.stop() } catch (_: IllegalStateException) { }
        rec.release()
        recorder = null
        recordThread = null
        recordFile = null
        return file
    }

    private fun recordStop(result: MethodChannel.Result) {
        val file = stopRecorder()
        if (file == null) {
            result.error("empty", "nothing is recording", null)
            return
        }
        writeWavHeader(file)
        result.success(file.absolutePath)
    }

    private fun recordCancel() {
        stopRecorder()?.delete()
    }

    private fun writeWavHeader(file: File) {
        val dataLen = (file.length() - 44).coerceAtLeast(0)
        RandomAccessFile(file, "rw").use { f ->
            fun le32(v: Long) = byteArrayOf(v.toByte(), (v shr 8).toByte(), (v shr 16).toByte(), (v shr 24).toByte())
            fun le16(v: Int) = byteArrayOf(v.toByte(), (v shr 8).toByte())
            f.seek(0)
            f.write("RIFF".toByteArray()); f.write(le32(36 + dataLen)); f.write("WAVE".toByteArray())
            f.write("fmt ".toByteArray()); f.write(le32(16)); f.write(le16(1)); f.write(le16(1))
            f.write(le32(sampleRate.toLong())); f.write(le32(sampleRate * 2L)); f.write(le16(2)); f.write(le16(16))
            f.write("data".toByteArray()); f.write(le32(dataLen))
        }
    }

    // --- camera (system camera app; no CAMERA permission needed) -------------

    private fun takePhoto(result: MethodChannel.Result) {
        val file = File(activity.cacheDir, "harbor-photo-${UUID.randomUUID()}.jpg")
        val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.capture", file)
        val intent = Intent(MediaStore.ACTION_IMAGE_CAPTURE)
            .putExtra(MediaStore.EXTRA_OUTPUT, uri)
            .addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        if (intent.resolveActivity(activity.packageManager) == null) {
            result.error("unsupported", "no camera app on this device", null)
            return
        }
        photoResult = result
        photoFile = file
        activity.startActivityForResult(intent, reqPhoto)
    }

    // --- on-device transcription of a recording made here --------------------

    private fun destroyRecognizer() {
        recognizer?.destroy()
        recognizer = null
    }

    private fun transcribe(path: String?, locale: String?, result: MethodChannel.Result) {
        if (!onDeviceSpeech()) {
            result.error("unsupported", "no on-device speech recognizer on this device (needs Android 13+)", null)
            return
        }
        val file = path?.let(::File)
        val pcm = file?.takeIf { it.isFile }?.let(::pcmOf)
        if (pcm == null) {
            result.error("format", "only recordings made in Harbor (16 kHz mono WAV) can be transcribed on this device", null)
            return
        }
        val pipe = ParcelFileDescriptor.createPipe()
        Thread {
            ParcelFileDescriptor.AutoCloseOutputStream(pipe[1]).use { it.write(pcm) }
        }.start()
        activity.runOnUiThread {
            destroyRecognizer()
            val rec = SpeechRecognizer.createOnDeviceSpeechRecognizer(activity)
            recognizer = rec
            var done = false
            fun finish(block: () -> Unit) {
                if (done) return
                done = true
                destroyRecognizer()
                block()
            }
            rec.setRecognitionListener(object : RecognitionListener {
                override fun onResults(results: Bundle?) {
                    val text = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull() ?: ""
                    finish { result.success(mapOf("engine" to "android-on-device-speech", "locale" to (locale ?: ""), "text" to text)) }
                }
                override fun onError(error: Int) {
                    finish { result.error("speech", "the on-device recognizer failed (code $error)", null) }
                }
                override fun onReadyForSpeech(p: Bundle?) {}
                override fun onBeginningOfSpeech() {}
                override fun onRmsChanged(v: Float) {}
                override fun onBufferReceived(b: ByteArray?) {}
                override fun onEndOfSpeech() {}
                override fun onPartialResults(p: Bundle?) {}
                override fun onEvent(t: Int, p: Bundle?) {}
            })
            val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
                .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE, pipe[0])
                .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_CHANNEL_COUNT, 1)
                .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
                .putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_SAMPLING_RATE, sampleRate)
            if (!locale.isNullOrEmpty()) {
                intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE, locale.replace('_', '-'))
            }
            rec.startListening(intent)
        }
    }

    /** The PCM payload of a 16 kHz mono PCM16 WAV, or null for anything else. */
    private fun pcmOf(file: File): ByteArray? {
        val b = file.readBytes()
        if (b.size < 44 || String(b, 0, 4) != "RIFF" || String(b, 8, 4) != "WAVE") return null
        fun le16(o: Int) = (b[o].toInt() and 0xFF) or ((b[o + 1].toInt() and 0xFF) shl 8)
        fun le32(o: Int) = le16(o) or (le16(o + 2) shl 16)
        var o = 12
        var ok = false
        while (o + 8 <= b.size) {
            val id = String(b, o, 4)
            val len = le32(o + 4)
            if (id == "fmt " && o + 8 + 16 <= b.size) {
                ok = le16(o + 8) == 1 && le16(o + 10) == 1 && le32(o + 12) == sampleRate && le16(o + 22) == 16
            } else if (id == "data") {
                val end = minOf(b.size, o + 8 + len)
                return if (ok) b.copyOfRange(o + 8, end) else null
            }
            o += 8 + len + (len and 1)
        }
        return null
    }
}
