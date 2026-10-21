package com.usbwebcam.app.camera

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Rect
import android.hardware.camera2.*
import android.hardware.camera2.params.OutputConfiguration
import android.hardware.camera2.params.SessionConfiguration
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.util.Range
import android.util.Log
import android.view.Surface
import com.usbwebcam.app.media.H264SurfaceEncoder
import com.usbwebcam.app.media.YuvFrameSource
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class Camera2Controller(private val context:Context,private val onFrameEncoded:(Byte,Long,ByteArray,Boolean)->Unit) {
    private val manager=context.getSystemService(Context.CAMERA_SERVICE) as CameraManager
    private val thread=HandlerThread("WebcamCamera",android.os.Process.THREAD_PRIORITY_DISPLAY).apply{start()}
    private val handler=Handler(thread.looper)
    private var device:CameraDevice?=null
    private var session:CameraCaptureSession?=null
    private var encoder:H264SurfaceEncoder?=null
    private var images:YuvFrameSource?=null
    @Volatile var currentTransport="h264";private set
    @Volatile var lastFrameMs=android.os.SystemClock.elapsedRealtime();private set
    private var request:CaptureRequest.Builder?=null
    private var generation=0
    val isStreaming=AtomicBoolean(false)
    @Volatile var lastError:String?=null;private set
    @Volatile var currentCameraId="0";private set
    @Volatile var currentWidth=1920;private set
    @Volatile var currentHeight=1080;private set
    @Volatile var currentFps=30;private set
    @Volatile var currentBitrate=15_000_000;private set
    @Volatile var currentZoomRatio=1f;private set
    @Volatile var currentExposureComp=0;private set
    @Volatile var isTorchOn=false;private set

    @SuppressLint("MissingPermission")
    @Synchronized fun startCameraStream(cameraId:String,width:Int,height:Int,fps:Int,bitrate:Int,transport:String="h264"):Boolean {
        val done=CountDownLatch(1)
        handler.post {
            if(isStreaming.get()&&currentCameraId==cameraId&&currentWidth==width&&currentHeight==height&&currentFps==fps&&currentTransport==transport){
                if(currentBitrate!=bitrate){try{encoder?.setBitrate(bitrate);currentBitrate=bitrate}catch(e:Exception){lastError=e.message;done.countDown();return@post}}
                encoder?.requestKeyFrame();done.countDown();return@post
            }
            generation++;val token=generation;isStreaming.set(false);lastError=null
            closeSession()
            val switched=currentCameraId!=cameraId
            if(switched){device?.close();device=null;currentZoomRatio=1f;currentExposureComp=0;isTorchOn=false}
            currentTransport=transport;lastFrameMs=android.os.SystemClock.elapsedRealtime();currentCameraId=cameraId;currentWidth=width;currentHeight=height;currentFps=fps;currentBitrate=bitrate
            fun fail(message:String){lastError=message;isStreaming.set(false);Log.e("WebcamCamera",message);done.countDown()}
            try{
                val surface:Surface
                if(transport=="h264"){
                    val next=H264SurfaceEncoder(width,height,fps,bitrate,{pts,data,key->lastFrameMs=android.os.SystemClock.elapsedRealtime();onFrameEncoded(1,pts,data,key)}){w,h,f->currentWidth=w;currentHeight=h;currentFps=f}
                    encoder=next
                    if(!next.start()){fail("This camera mode could not start. Try a lower resolution.");return@post}
                    surface=next.inputSurface?:run{fail("Camera encoder surface is unavailable");return@post}
                }else{
                    val next=YuvFrameSource(width,height,transport){type,pts,data->lastFrameMs=android.os.SystemClock.elapsedRealtime();onFrameEncoded(type,pts,data,true)}
                    images=next;surface=next.surface
                }
                fun configure(cam:CameraDevice){
                    if(token!=generation){return}
                    try{
                        val chars=manager.getCameraCharacteristics(cam.id)
                        val ranges=chars.get(CameraCharacteristics.CONTROL_AE_AVAILABLE_TARGET_FPS_RANGES).orEmpty()
                        val range=ranges.firstOrNull{it.lower==currentFps&&it.upper==currentFps}?:ranges.filter{it.upper==currentFps}.maxByOrNull{it.lower}?:ranges.filter{it.contains(currentFps)}.minByOrNull{it.upper-it.lower}
                        // The preview template can enable temporal photography processing.
                        // Use the video pipeline for independent frames as well as H264.
                        val builder=cam.createCaptureRequest(CameraDevice.TEMPLATE_RECORD).apply{
                            addTarget(surface);set(CaptureRequest.CONTROL_MODE,CaptureRequest.CONTROL_MODE_AUTO)
                            val af=(chars.get(CameraCharacteristics.CONTROL_AF_AVAILABLE_MODES) ?: intArrayOf())
                            if(af.contains(CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_VIDEO))set(CaptureRequest.CONTROL_AF_MODE,CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_VIDEO)
                            set(CaptureRequest.CONTROL_AE_MODE,CaptureRequest.CONTROL_AE_MODE_ON)
                            if(range!=null)set(CaptureRequest.CONTROL_AE_TARGET_FPS_RANGE,range)
                            set(CaptureRequest.CONTROL_ENABLE_ZSL,false)
                            // Video's fast noise filter also reduces encoder work in dim
                            // scenes; independent JPEG frames prefer minimal ISP processing.
                            val nr=if(transport=="h264")CaptureRequest.NOISE_REDUCTION_MODE_FAST else CaptureRequest.NOISE_REDUCTION_MODE_OFF
                            val edge=if(transport=="h264")CaptureRequest.EDGE_MODE_FAST else CaptureRequest.EDGE_MODE_OFF
                            if(chars.get(CameraCharacteristics.NOISE_REDUCTION_AVAILABLE_NOISE_REDUCTION_MODES)?.contains(nr)==true)set(CaptureRequest.NOISE_REDUCTION_MODE,nr)
                            if(chars.get(CameraCharacteristics.EDGE_AVAILABLE_EDGE_MODES)?.contains(edge)==true)set(CaptureRequest.EDGE_MODE,edge)
                            set(CaptureRequest.CONTROL_VIDEO_STABILIZATION_MODE,CaptureRequest.CONTROL_VIDEO_STABILIZATION_MODE_OFF)
                            if(chars.get(CameraCharacteristics.STATISTICS_INFO_AVAILABLE_FACE_DETECT_MODES)?.contains(CaptureRequest.STATISTICS_FACE_DETECT_MODE_OFF)==true)set(CaptureRequest.STATISTICS_FACE_DETECT_MODE,CaptureRequest.STATISTICS_FACE_DETECT_MODE_OFF)
                        }
                        request=builder;applyControls(builder)
                        val callback=object:CameraCaptureSession.StateCallback(){
                            override fun onConfigured(s:CameraCaptureSession){
                                if(token!=generation){s.close();return}
                                try{
                                    session=s
                                    var samples=0
                                    val captureClock=chars.get(CameraCharacteristics.SENSOR_INFO_TIMESTAMP_SOURCE)
                                    Log.i("WebcamCamera","Sensor timestamp source="+captureClock)
                                    val timing=object:CameraCaptureSession.CaptureCallback(){
                                        override fun onCaptureStarted(session:CameraCaptureSession,request:CaptureRequest,timestamp:Long,frameNumber:Long){
                                            if(++samples%150==0){
                                                val boot=android.os.SystemClock.elapsedRealtimeNanos()
                                                val age=minOf(kotlin.math.abs(boot-timestamp),kotlin.math.abs(System.nanoTime()-timestamp))/1000000
                                                Log.i("WebcamCamera","Sensor callback ageMs="+age)
                                            }
                                        }
                                    }
                                    s.setRepeatingRequest(builder.build(),timing,handler);isStreaming.set(true);done.countDown()
                                }
                                catch(e:Exception){fail(e.message?:"Could not start camera capture")}
                            }
                            override fun onConfigureFailed(s:CameraCaptureSession){s.close();if(token==generation)fail("Camera rejected this mode. Try a lower resolution.")}
                        }
                        cam.createCaptureSession(SessionConfiguration(SessionConfiguration.SESSION_REGULAR,listOf(OutputConfiguration(surface)),java.util.concurrent.Executor{handler.post(it)},callback))
                    }catch(e:Exception){fail(e.message?:"Camera configuration failed")}
                }
                val existing=device
                if(existing!=null&&existing.id==cameraId)configure(existing)
                else manager.openCamera(cameraId,object:CameraDevice.StateCallback(){
                    override fun onOpened(cam:CameraDevice){if(token!=generation){cam.close();return};device=cam;configure(cam)}
                    override fun onDisconnected(cam:CameraDevice){cam.close();if(device===cam){device=null;fail("Camera disconnected")}}
                    override fun onError(cam:CameraDevice,error:Int){cam.close();if(device===cam){device=null;fail("Camera is unavailable (code $error). Close other camera apps and retry.")}}
                },handler)
            }catch(e:Exception){fail(e.message?:"Could not open camera")}
        }
        if(!done.await(7,TimeUnit.SECONDS)){lastError="Camera did not start in time";handler.post{generation++;isStreaming.set(false);closeSession();device?.close();device=null};return false}
        return isStreaming.get()
    }
    private fun applyControls(builder:CaptureRequest.Builder){
        val chars=manager.getCameraCharacteristics(currentCameraId)
        if(Build.VERSION.SDK_INT>=30){
            val range=chars.get(CameraCharacteristics.CONTROL_ZOOM_RATIO_RANGE)
            if(range!=null){currentZoomRatio=currentZoomRatio.coerceIn(range.lower,range.upper);builder.set(CaptureRequest.CONTROL_ZOOM_RATIO,currentZoomRatio)}
            else applyCrop(builder,chars)
        }else applyCrop(builder,chars)
        val ev=chars.get(CameraCharacteristics.CONTROL_AE_COMPENSATION_RANGE)
        if(ev!=null){currentExposureComp=currentExposureComp.coerceIn(ev.lower,ev.upper);builder.set(CaptureRequest.CONTROL_AE_EXPOSURE_COMPENSATION,currentExposureComp)}
        if(chars.get(CameraCharacteristics.FLASH_INFO_AVAILABLE)==true)builder.set(CaptureRequest.FLASH_MODE,if(isTorchOn)CaptureRequest.FLASH_MODE_TORCH else CaptureRequest.FLASH_MODE_OFF)
    }
    private fun applyCrop(builder:CaptureRequest.Builder,chars:CameraCharacteristics){
        val area=chars.get(CameraCharacteristics.SENSOR_INFO_ACTIVE_ARRAY_SIZE)?:return
        val max=chars.get(CameraCharacteristics.SCALER_AVAILABLE_MAX_DIGITAL_ZOOM)?:1f
        currentZoomRatio=currentZoomRatio.coerceIn(1f,max)
        val w=(area.width()/currentZoomRatio).toInt();val h=(area.height()/currentZoomRatio).toInt()
        builder.set(CaptureRequest.SCALER_CROP_REGION,Rect(area.centerX()-w/2,area.centerY()-h/2,area.centerX()+w/2,area.centerY()+h/2))
    }
    private fun updateControls(){handler.post{try{request?.let{applyControls(it);session?.setRepeatingRequest(it.build(),null,handler)}}catch(e:Exception){lastError=e.message}}}
    fun setZoom(ratio:Float){currentZoomRatio=ratio;updateControls()}
    fun setExposureCompensation(value:Int){currentExposureComp=value;updateControls()}
    fun setTorch(enabled:Boolean){isTorchOn=enabled;updateControls()}
    fun requestKeyFrame(){handler.post{encoder?.requestKeyFrame()}}
    private fun closeSession(){try{session?.close()}catch(_:Exception){};session=null;request=null;encoder?.release();encoder=null;images?.close();images=null}
    fun stopStream(){val done=CountDownLatch(1);handler.post{generation++;isStreaming.set(false);closeSession();device?.close();device=null;isTorchOn=false;done.countDown()};done.await(2,TimeUnit.SECONDS)}
    fun release(){stopStream();thread.quitSafely()}
}
