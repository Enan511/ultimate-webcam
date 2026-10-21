package com.usbwebcam.app.camera

import android.content.Context
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CameraMetadata
import android.hardware.camera2.params.StreamConfigurationMap
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.os.Build
import android.util.Range
import android.util.Size
import org.json.JSONArray
import org.json.JSONObject

data class CameraMode(
    val width: Int,
    val height: Int,
    val fps: Int
)

data class ZoomRange(
    val min: Float,
    val max: Float
)

data class CameraDescriptor(
    val id: String,
    val name: String,
    val lensFacing: String,
    val sensorOrientation: Int,
    val focalLength: Float,
    val aperture: Float,
    val hasFlash: Boolean,
    val opticalStabilization: Boolean,
    val physicalIds: List<String>,
    val modes: List<CameraMode>,
    val zoomRange: ZoomRange,
    val exposureRange: Range<Int>
)

data class CapabilitiesManifest(
    val type: String = "CAPABILITIES",
    val model: String = Build.MODEL,
    val manufacturer: String = Build.MANUFACTURER,
    val soc: String = Build.HARDWARE,
    val cameras: List<CameraDescriptor>,
    val audioSupported: Boolean = true,
    val maxSupportedBitrate: Int
) {
    fun toJsonString(): String {
        val root = JSONObject()
        root.put("type", type)
        root.put("model", model)
        root.put("manufacturer", manufacturer)
        root.put("soc", soc)
        root.put("audioSupported", audioSupported)
        root.put("maxSupportedBitrate", maxSupportedBitrate)

        val camArray = JSONArray()
        for (cam in cameras) {
            val camObj = JSONObject()
            camObj.put("id", cam.id)
            camObj.put("name", cam.name)
            camObj.put("lensFacing", cam.lensFacing)
            camObj.put("sensorOrientation", cam.sensorOrientation)
            camObj.put("focalLength", cam.focalLength.toDouble())
            camObj.put("aperture", cam.aperture.toDouble())
            camObj.put("hasFlash", cam.hasFlash)
            camObj.put("opticalStabilization", cam.opticalStabilization)

            val physArray = JSONArray()
            cam.physicalIds.forEach { physArray.put(it) }
            camObj.put("physicalIds", physArray)

            val modesArray = JSONArray()
            for (m in cam.modes) {
                val mObj = JSONObject()
                mObj.put("width", m.width)
                mObj.put("height", m.height)
                mObj.put("fps", m.fps)
                modesArray.put(mObj)
            }
            camObj.put("modes", modesArray)

            val zoomObj = JSONObject()
            zoomObj.put("min", cam.zoomRange.min.toDouble())
            zoomObj.put("max", cam.zoomRange.max.toDouble())
            camObj.put("zoomRange", zoomObj)
            camObj.put("exposureRange", JSONObject().put("min", cam.exposureRange.lower).put("max", cam.exposureRange.upper))

            camArray.put(camObj)
        }
        root.put("cameras", camArray)
        return root.toString()
    }
}

object LensEnumerator {

    private val TARGET_RESOLUTIONS = listOf(
        Size(3840, 2160), // 4K UHD
        Size(1920, 1080), // 1080p FHD
        Size(1280, 720),  // 720p HD
        Size(640, 480),   // 480p SD
        Size(320, 240)    // Reduced processing load on slower phones
    )

    fun enumerateCapabilities(context: Context): CapabilitiesManifest {
        val cameraManager = context.getSystemService(Context.CAMERA_SERVICE) as CameraManager
        val hardwareCodecCapabilities = queryH264EncoderCapabilities()

        val descriptors = mutableListOf<CameraDescriptor>()

        for (cameraId in cameraManager.cameraIdList) {
            val chars = cameraManager.getCameraCharacteristics(cameraId)
            val lensFacing = chars.get(CameraCharacteristics.LENS_FACING) ?: continue

            val facingStr = when (lensFacing) {
                CameraMetadata.LENS_FACING_FRONT -> "FRONT"
                CameraMetadata.LENS_FACING_BACK -> "BACK"
                else -> "EXTERNAL"
            }

            // Check if this is a logical multi-camera
            val capabilities = chars.get(CameraCharacteristics.REQUEST_AVAILABLE_CAPABILITIES) ?: intArrayOf()
            val isLogicalMultiCam = capabilities.contains(CameraMetadata.REQUEST_AVAILABLE_CAPABILITIES_LOGICAL_MULTI_CAMERA)
            val physicalIds = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && isLogicalMultiCam) {
                chars.physicalCameraIds.toList()
            } else {
                listOf(cameraId)
            }

            // Only advertise IDs that CameraManager can actually open.
            val descriptor = buildDescriptorForCamera(cameraId, facingStr, chars, physicalIds, hardwareCodecCapabilities, false)
            if (descriptor.modes.isNotEmpty()) descriptors.add(descriptor)
        }

        val maxBitrate = hardwareCodecCapabilities?.videoCapabilities?.bitrateRange?.upper ?: 50000000

        return CapabilitiesManifest(
            cameras = descriptors,
            audioSupported = true,
            maxSupportedBitrate = maxBitrate
        )
    }

    private fun buildDescriptorForCamera(
        cameraId: String,
        facingStr: String,
        chars: CameraCharacteristics,
        physicalIds: List<String>,
        hardwareCodecCaps: MediaCodecInfo.CodecCapabilities?,
        isPhysicalSubCamera: Boolean
    ): CameraDescriptor {
        val focalLengths = chars.get(CameraCharacteristics.LENS_INFO_AVAILABLE_FOCAL_LENGTHS)
        val focalLength = focalLengths?.firstOrNull() ?: 24.0f
        val apertures = chars.get(CameraCharacteristics.LENS_INFO_AVAILABLE_APERTURES)
        val aperture = apertures?.firstOrNull() ?: 1.8f
        val hasFlash = chars.get(CameraCharacteristics.FLASH_INFO_AVAILABLE) ?: false

        val oisModes = chars.get(CameraCharacteristics.LENS_INFO_AVAILABLE_OPTICAL_STABILIZATION)
        val hasOis = oisModes != null && oisModes.contains(CameraMetadata.LENS_OPTICAL_STABILIZATION_MODE_ON)

        // Friendly Name derivation
        val name = when {
            facingStr == "FRONT" -> "Front Selfie (${focalLength.toInt()}mm f/$aperture)"
            focalLength < 2.5f -> "Rear Ultra-Wide (${focalLength.toInt()}mm f/$aperture)"
            focalLength > 7.0f -> "Rear 2x Telephoto (${focalLength.toInt()}mm f/$aperture)"
            isPhysicalSubCamera -> "Rear Physical Sensor $cameraId (${focalLength.toInt()}mm f/$aperture)"
            else -> "Rear Primary Wide (${focalLength.toInt()}mm f/$aperture)"
        }

        // Query supported modes
        val streamMap = chars.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
        val supportedModes = mutableListOf<CameraMode>()

        if (streamMap != null) {
            val outputSizes = streamMap.getOutputSizes(MediaCodec::class.java) ?: emptyArray()
            val outputSet = outputSizes.toSet()

            for (target in TARGET_RESOLUTIONS) {
                if (outputSet.contains(target)) {
                    // Check if hardware encoder can encode this size
                    if (hardwareCodecCaps != null && !hardwareCodecCaps.videoCapabilities.isSizeSupported(target.width, target.height)) {
                        continue
                    }

                    // Check supported frame rates (typically 30fps standard, check if 60fps is supported)
                    val minDuration = streamMap.getOutputMinFrameDuration(MediaCodec::class.java, target)
                    val maxSensorFps = if (minDuration > 0) (1_000_000_000L / minDuration).toInt() else 30

                    val aeRanges = chars.get(CameraCharacteristics.CONTROL_AE_AVAILABLE_TARGET_FPS_RANGES).orEmpty()
                    val supportedFpsList = mutableListOf<Int>()
                    if (maxSensorFps >= 29 && aeRanges.any { it.upper >= 30 } && hardwareCodecCaps?.videoCapabilities?.areSizeAndRateSupported(target.width, target.height, 30.0) != false) supportedFpsList.add(30)
                    if (maxSensorFps >= 58 && target.height <= 1080 && aeRanges.any { it.upper >= 60 }) {
                        val encoderRates = hardwareCodecCaps?.videoCapabilities?.getSupportedFrameRatesFor(target.width, target.height)
                        if (encoderRates == null || encoderRates.contains(60.0)) {
                            supportedFpsList.add(0, 60) // Add 60 first as preferred
                        }
                    }

                    for (fps in supportedFpsList) {
                        supportedModes.add(CameraMode(target.width, target.height, fps))
                    }
                }
            }
        }

        // Zoom range
        val zoomRange = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val r = chars.get(CameraCharacteristics.CONTROL_ZOOM_RATIO_RANGE)
            if (r != null) ZoomRange(r.lower, r.upper) else ZoomRange(1.0f, chars.get(CameraCharacteristics.SCALER_AVAILABLE_MAX_DIGITAL_ZOOM) ?: 1.0f)
        } else {
            val maxDigital = chars.get(CameraCharacteristics.SCALER_AVAILABLE_MAX_DIGITAL_ZOOM) ?: 1.0f
            ZoomRange(1.0f, maxDigital.coerceAtLeast(1.0f))
        }

        return CameraDescriptor(
            id = cameraId,
            name = name,
            lensFacing = facingStr,
            sensorOrientation = chars.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 0,
            focalLength = focalLength,
            aperture = aperture,
            hasFlash = hasFlash,
            opticalStabilization = hasOis,
            physicalIds = physicalIds,
            modes = supportedModes,
            zoomRange = zoomRange,
            exposureRange = chars.get(CameraCharacteristics.CONTROL_AE_COMPENSATION_RANGE) ?: Range(0, 0)
        )
    }

    private fun queryH264EncoderCapabilities(): MediaCodecInfo.CodecCapabilities? {
        val codecList = MediaCodecList(MediaCodecList.REGULAR_CODECS)
        for (info in codecList.codecInfos) {
            if (!info.isEncoder) continue
            val types = info.supportedTypes
            for (type in types) {
                if (type.equals("video/avc", ignoreCase = true)) {
                    return info.getCapabilitiesForType(type)
                }
            }
        }
        return null
    }
}
