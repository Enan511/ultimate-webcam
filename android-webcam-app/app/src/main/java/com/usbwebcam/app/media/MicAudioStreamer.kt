package com.usbwebcam.app.media

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.util.Log
import java.util.concurrent.atomic.AtomicBoolean

class MicAudioStreamer(
    private val onAudioChunk: (ptsUs: Long, data: ByteArray) -> Unit
) {
    companion object {
        private const val TAG = "MicAudioStreamer"
        const val SAMPLE_RATE_HZ = 48000
        const val CHANNEL_CONFIG = AudioFormat.CHANNEL_IN_MONO
        const val AUDIO_FORMAT = AudioFormat.ENCODING_PCM_16BIT
        // 10ms frame = 480 samples * 2 bytes per sample = 960 bytes
        const val SAMPLES_PER_CHUNK = 480
        const val CHUNK_SIZE_BYTES = SAMPLES_PER_CHUNK * 2
    }

    val isRunning = AtomicBoolean(false)
    private var audioRecord: AudioRecord? = null
    private var recordingThread: Thread? = null

    @SuppressLint("MissingPermission")
    @Synchronized
    fun start(): Boolean {
        if (isRunning.get()) return true

        val minBufSize = AudioRecord.getMinBufferSize(SAMPLE_RATE_HZ, CHANNEL_CONFIG, AUDIO_FORMAT)
        val bufferSize = (minBufSize * 2).coerceAtLeast(CHUNK_SIZE_BYTES * 4)

        return try {
            val record = AudioRecord(
                MediaRecorder.AudioSource.MIC,
                SAMPLE_RATE_HZ,
                CHANNEL_CONFIG,
                AUDIO_FORMAT,
                bufferSize
            )

            if (record.state != AudioRecord.STATE_INITIALIZED) {
                Log.e(TAG, "AudioRecord initialization failed")
                record.release()
                return false
            }

            record.startRecording()
            audioRecord = record
            isRunning.set(true)

            recordingThread = Thread({
                val buffer = ByteArray(CHUNK_SIZE_BYTES)
                while (isRunning.get()) {
                    val rec = audioRecord ?: break
                    var readTotal = 0
                    while (readTotal < CHUNK_SIZE_BYTES && isRunning.get()) {
                        val read = rec.read(buffer, readTotal, CHUNK_SIZE_BYTES - readTotal)
                        if (read > 0) {
                            readTotal += read
                        } else if (read < 0) {
                            Log.w(TAG, "AudioRecord read error: $read")
                            break
                        }
                    }

                    if (readTotal == CHUNK_SIZE_BYTES && isRunning.get()) {
                        val ptsUs = System.nanoTime() / 1000
                        val copy = ByteArray(CHUNK_SIZE_BYTES)
                        System.arraycopy(buffer, 0, copy, 0, CHUNK_SIZE_BYTES)
                        onAudioChunk(ptsUs, copy)
                    }
                }
            }, "MicAudioStreamer").apply {
                priority = Thread.NORM_PRIORITY + 2
                start()
            }

            Log.i(TAG, "Audio stream started (48kHz, 16-bit Mono, 10ms chunks)")
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start audio stream", e)
            isRunning.set(false)
            false
        }
    }

    @Synchronized
    fun stop() {
        if (!isRunning.getAndSet(false)) return

        try {
            audioRecord?.stop()
        } catch (_: Exception) {}
        try {
            audioRecord?.release()
        } catch (_: Exception) {}
        audioRecord = null

        try {
            recordingThread?.interrupt()
            recordingThread?.join(500)
        } catch (_: Exception) {}
        recordingThread = null

        Log.i(TAG, "Audio stream stopped and microphone hardware released")
    }
}
