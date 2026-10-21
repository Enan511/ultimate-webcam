package com.usbwebcam.app.media

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.os.Build
import android.util.Log
import android.view.Surface
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

class H264SurfaceEncoder(
    private var width: Int,
    private var height: Int,
    private var fps: Int,
    private var bitrate: Int,
    private val onFrameEncoded: (ptsUs: Long, payload: ByteArray, isKeyFrame: Boolean) -> Unit,
    private val onFallbackTriggered: (newWidth: Int, newHeight: Int, newFps: Int) -> Unit
) {
    companion object {
        private const val TAG = "H264SurfaceEncoder"
        private const val MIME_TYPE = "video/avc"
        private const val TIMEOUT_USEC = 10_000L
    }

    private var mediaCodec: MediaCodec? = null
    var inputSurface: Surface? = null
        private set

    private val isRunning = AtomicBoolean(false)
    private var drainThread: Thread? = null

    // Cache SPS/PPS NALUs from BUFFER_FLAG_CODEC_CONFIG to prepend to every keyframe
    private var cachedSpsPps: ByteArray? = null
    private var diagnosticFrames=0

    fun start(): Boolean {
        return try {
            initCodec(width, height, fps, bitrate)
            startDrainThread()
            true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to initialize encoder for ${width}x${height}@$fps, attempting fallback", e)
            release()
            if (width > 1920 || height > 1080) {
                // Fall back from 4K to 1080p
                width = 1920
                height = 1080
                fps = 30
                bitrate = 12_000_000
                onFallbackTriggered(width, height, fps)
                try {
                    initCodec(width, height, fps, bitrate)
                    startDrainThread()
                    true
                } catch (fallbackEx: Exception) {
                    Log.e(TAG, "1080p fallback also failed", fallbackEx)
                    false
                }
            } else {
                false
            }
        }
    }

    private fun initCodec(w: Int, h: Int, f: Int, b: Int) {
        val format = MediaFormat.createVideoFormat(MIME_TYPE, w, h).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            setInteger(MediaFormat.KEY_BIT_RATE, b)
            setInteger(MediaFormat.KEY_FRAME_RATE, f)
            setInteger(MediaFormat.KEY_OPERATING_RATE, f)
            if(Build.VERSION.SDK_INT>=29){
                val headroomFps=if(Build.HARDWARE.startsWith("mt67")&&w>=1920&&f==30)28f else f.toFloat()
                setFloat(MediaFormat.KEY_MAX_FPS_TO_ENCODER,headroomFps)
            }
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
            setInteger(MediaFormat.KEY_BITRATE_MODE, MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_CBR)

            // Strictly enforce Baseline Profile for 100% OpenH264 compatibility
            setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline)
            setInteger(MediaFormat.KEY_LEVEL, if (w > 1920 || h > 1080) MediaCodecInfo.CodecProfileLevel.AVCLevel51 else if (f > 30) MediaCodecInfo.CodecProfileLevel.AVCLevel42 else MediaCodecInfo.CodecProfileLevel.AVCLevel4)

            // Low-latency zero B-frame pipeline flags
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                setInteger(MediaFormat.KEY_LATENCY, 0)
                setInteger(MediaFormat.KEY_PRIORITY, 0) // Real-time priority
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                setInteger(MediaFormat.KEY_MAX_B_FRAMES, 0)
            }
        }

        val codec = MediaCodec.createEncoderByType(MIME_TYPE)
        mediaCodec = codec
        val complexity=codec.codecInfo.getCapabilitiesForType(MIME_TYPE).encoderCapabilities.complexityRange
        format.setInteger(MediaFormat.KEY_COMPLEXITY,complexity.lower)
        if(Build.VERSION.SDK_INT>=31){
            val lowLatencyKeys=listOf("vendor.mtk.ext.venc.wfd.feature-on","vendor.mtk.ext.venc.wfd.low-latency-enabled")
                .filter{it in codec.supportedVendorParameters}
            for(key in lowLatencyKeys){
                val descriptor=codec.getParameterDescriptor(key)
                if(descriptor?.type==MediaFormat.TYPE_INTEGER){format.setInteger(key,1);Log.i(TAG,"Enabled supported realtime control: "+key)}
            }
            if(lowLatencyKeys.isNotEmpty())codec.subscribeToVendorParameters(lowLatencyKeys)
        }
        codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
        inputSurface = codec.createInputSurface()
        codec.start()

        mediaCodec = codec
        isRunning.set(true)
        Log.i(TAG, "Hardware H.264 Surface Encoder started at ${w}x${h} @ ${f}fps, ${b / 1_000_000} Mbps")
    }

    private fun startDrainThread() {
        drainThread = Thread({
            val bufferInfo = MediaCodec.BufferInfo()
            while (isRunning.get()) {
                val codec = mediaCodec ?: break
                try {
                    val outputBufferId = codec.dequeueOutputBuffer(bufferInfo, TIMEOUT_USEC)
                    if (outputBufferId >= 0) {
                        val outputBuffer = codec.getOutputBuffer(outputBufferId)
                        if (outputBuffer != null) {
                            processOutputBuffer(outputBuffer, bufferInfo)
                        }
                        codec.releaseOutputBuffer(outputBufferId, false)
                    } else if (outputBufferId == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                        val newFormat = codec.outputFormat
                        Log.i(TAG, "Encoder output format changed: $newFormat")
                        // Extract csd-0 (SPS) and csd-1 (PPS) if present
                        val sps = newFormat.getByteBuffer("csd-0")
                        val pps = newFormat.getByteBuffer("csd-1")
                        if (sps != null && pps != null) {
                            val baos = ByteArrayOutputStream()
                            val spsBytes = ByteArray(sps.remaining()).also { sps.get(it) }
                            val ppsBytes = ByteArray(pps.remaining()).also { pps.get(it) }
                            baos.write(spsBytes)
                            baos.write(ppsBytes)
                            cachedSpsPps = baos.toByteArray()
                            Log.i(TAG, "Cached SPS/PPS from output format (${cachedSpsPps?.size} bytes)")
                        }
                    }
                } catch (e: Exception) {
                    if (isRunning.get()) {
                        Log.w(TAG, "Exception during drain loop", e)
                    }
                }
            }
        }, "H264EncoderDrain").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    private fun processOutputBuffer(buffer: ByteBuffer, info: MediaCodec.BufferInfo) {
        if (info.size <= 0) return

        buffer.position(info.offset)
        buffer.limit(info.offset + info.size)

        val isConfig = (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0
        val isKeyFrame = (info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0

        if (isConfig) {
            // Save SPS/PPS header bytes
            val configBytes = ByteArray(info.size)
            buffer.get(configBytes)
            cachedSpsPps = configBytes
            Log.i(TAG, "Cached SPS/PPS from BUFFER_FLAG_CODEC_CONFIG (${configBytes.size} bytes)")
            return
        }

        val frameData: ByteArray
        if (isKeyFrame) {
            // Prepend cached SPS/PPS if not already prepended
            val bodyBytes = ByteArray(info.size)
            buffer.get(bodyBytes)

            val spsPps = cachedSpsPps
            if (spsPps != null && !hasSpsPpsPrefix(bodyBytes)) {
                val combined = ByteArray(spsPps.size + bodyBytes.size)
                System.arraycopy(spsPps, 0, combined, 0, spsPps.size)
                System.arraycopy(bodyBytes, 0, combined, spsPps.size, bodyBytes.size)
                frameData = combined
            } else {
                frameData = bodyBytes
            }
        } else {
            frameData = ByteArray(info.size)
            buffer.get(frameData)
        }

        // Camera timestamps may use monotonic or boot time depending on the HAL.
        // Normalize to Android boot time so the desktop can measure frame age.
        val bootUs=android.os.SystemClock.elapsedRealtimeNanos()/1000
        val monotonicUs=System.nanoTime()/1000
        val age=minOf(kotlin.math.abs(bootUs-info.presentationTimeUs),kotlin.math.abs(monotonicUs-info.presentationTimeUs))
        if(++diagnosticFrames%150==0)Log.i(TAG,"Encoder delivery ageMs="+age/1000)
        onFrameEncoded((bootUs-age).coerceAtLeast(0), frameData, isKeyFrame)
    }

    private fun hasSpsPpsPrefix(data: ByteArray): Boolean {
        if (data.size < 5) return false
        val limit = minOf(data.size - 4, 64)
        for (i in 0 until limit) {
            if (data[i] == 0.toByte() && data[i + 1] == 0.toByte()) {
                val nalPos = when {
                    i + 2 < data.size && data[i + 2] == 1.toByte() -> i + 3
                    i + 3 < data.size && data[i + 2] == 0.toByte() && data[i + 3] == 1.toByte() -> i + 4
                    else -> -1
                }
                if (nalPos != -1 && nalPos < data.size) {
                    val nalType = data[nalPos].toInt() and 0x1F
                    if (nalType == 7) return true
                }
            }
        }
        return false
    }

    fun setBitrate(value: Int) {
        val params = android.os.Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_VIDEO_BITRATE, value) }
        mediaCodec?.setParameters(params)
        bitrate = value
    }

    fun requestKeyFrame() {
        try {
            val params = android.os.Bundle().apply {
                putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0)
            }
            mediaCodec?.setParameters(params)
            Log.i(TAG, "Requested immediate sync frame from MediaCodec")
        } catch (e: Exception) {
            Log.w(TAG, "Failed to request sync frame", e)
        }
    }

    fun release() {
        isRunning.set(false)
        try {
            drainThread?.interrupt()
            drainThread?.join(500)
        } catch (_: Exception) {}
        drainThread = null

        try {
            mediaCodec?.stop()
        } catch (_: Exception) {}
        try {
            mediaCodec?.release()
        } catch (_: Exception) {}
        mediaCodec = null

        inputSurface?.release()
        inputSurface = null
        cachedSpsPps = null
        Log.i(TAG, "H.264 Surface Encoder released")
    }
}
