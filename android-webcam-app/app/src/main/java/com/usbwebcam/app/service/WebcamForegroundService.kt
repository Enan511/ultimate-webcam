package com.usbwebcam.app.service
import android.Manifest
import android.app.*
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.*
import android.util.Log
import android.view.OrientationEventListener
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import com.usbwebcam.app.MainActivity
import com.usbwebcam.app.camera.*
import com.usbwebcam.app.media.MicAudioStreamer
import com.usbwebcam.app.net.UsbSocketServer
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.json.JSONObject
import java.util.concurrent.atomic.AtomicLong

data class ServiceTelemetry(
 val transport:String="h264",val isClientConnected:Boolean=false,val isStreaming:Boolean=false,val activeCameraId:String="0",
 val autoOrientation:Boolean=true,val rotationDegrees:Int=0,
 val width:Int=0,val height:Int=0,val fps:Int=0,val bitrateMbps:Float=15f,val bytesTransmitted:Long=0,
 val isAudioActive:Boolean=false,val torchEnabled:Boolean=false,val zoomRatio:Float=1f,val exposureCompensation:Int=0,val error:String?=null
){
 fun json()=JSONObject().apply{
  put("transport",transport);put("isClientConnected",isClientConnected);put("isStreaming",isStreaming);put("activeCameraId",activeCameraId)
  put("autoOrientation",autoOrientation);put("rotationDegrees",rotationDegrees)
  put("width",width);put("height",height);put("fps",fps);put("bitrateMbps",bitrateMbps.toDouble())
  put("bytesTransmitted",bytesTransmitted);put("isAudioActive",isAudioActive);put("torchEnabled",torchEnabled)
  put("zoomRatio",zoomRatio.toDouble());put("exposureCompensation",exposureCompensation);put("error",error?:JSONObject.NULL)
 }
}
class WebcamForegroundService:Service(){
 companion object{const val ACTION_START="com.usbwebcam.app.ACTION_START";const val ACTION_STOP="com.usbwebcam.app.ACTION_STOP";private const val CHANNEL="webcam"}
 inner class LocalBinder:Binder(){fun getService()=this@WebcamForegroundService}
 private val binder=LocalBinder();private var wakeLock:PowerManager.WakeLock?=null
 private lateinit var manifest:CapabilitiesManifest
 private lateinit var camera:Camera2Controller
 private lateinit var mic:MicAudioStreamer
 private lateinit var server:UsbSocketServer
 private val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
 private val bytes=AtomicLong()
 private val _preview=MutableStateFlow<ByteArray?>(null);val preview=_preview.asStateFlow()
 @Volatile private var previewVisible=false
 fun setPreviewVisible(visible:Boolean){previewVisible=visible;if(!visible)_preview.value=null;if(initialized)server.sendPacket(6,0,byteArrayOf(if(visible)1 else 0))}
 private val _telemetry=MutableStateFlow(ServiceTelemetry());val telemetry=_telemetry.asStateFlow()
 @Volatile private var initialized=false
 @Volatile private var stopping=false
 @Volatile private var wantStreaming=true
 private var lastRecovery=0L
 private var orientationListener:OrientationEventListener?=null
 @Volatile private var deviceOrientation=0
 @Volatile private var autoOrientation=true
 private fun rotation():Int{
  val lens=manifest.cameras.find{it.id==camera.currentCameraId}?:return 0
  return if(autoOrientation)(lens.sensorOrientation+(if(lens.lensFacing=="FRONT")-deviceOrientation else deviceOrientation)+360)%360 else 0
 }
 private fun sendOrientation(){
  if(!initialized)return
  server.sendPacket(11,0,JSONObject().put("rotation",rotation()).toString().toByteArray(Charsets.UTF_8))
 }
 fun setAutoOrientation(enabled:Boolean){
  autoOrientation=enabled;getSharedPreferences("camera",MODE_PRIVATE).edit().putBoolean("autoOrientation",enabled).apply()
  sendOrientation();syncTelemetry()
 }
 override fun onBind(intent:Intent?):IBinder=binder
 override fun onCreate(){
  super.onCreate()
  val nm=getSystemService(NotificationManager::class.java)
  nm.createNotificationChannel(NotificationChannel(CHANNEL,"UWebcam connection",NotificationManager.IMPORTANCE_LOW).apply{setShowBadge(false)})
  promote(false)
  wakeLock=(getSystemService(Context.POWER_SERVICE) as PowerManager).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK,"UWebcam:camera").apply{acquire(12*60*60*1000L)}
  manifest=LensEnumerator.enumerateCapabilities(this)
  autoOrientation=getSharedPreferences("camera",MODE_PRIVATE).getBoolean("autoOrientation",true)
  server=UsbSocketServer(onControlCommand={handle(it)},onClientConnected={
   synchronized(this@WebcamForegroundService){if(!stopping){
    if(!camera.isStreaming.get())resume()
    _telemetry.update{it.copy(isClientConnected=true)}
    server.sendCapabilitiesManifest(manifest.toJsonString());sendOrientation();server.sendPacket(6,0,byteArrayOf(if(previewVisible)1 else 0));camera.requestKeyFrame()
   }}
  },onClientDisconnected={
   synchronized(this@WebcamForegroundService){if(!stopping&&!server.hasMediaClient()){
    _preview.value=null
    _telemetry.update{it.copy(isClientConnected=false,isAudioActive=false)}
    pause()
   }}
  },onPreview={jpeg->if(jpeg.isEmpty())_preview.value=null else if(previewVisible&&camera.isStreaming.get())_preview.value=jpeg},onNeedsKeyFrame={camera.requestKeyFrame()})
  camera=Camera2Controller(this){type,pts,payload,key->if(server.sendPacket(type,pts,payload,key))bytes.addAndGet(payload.size.toLong())}
  mic=MicAudioStreamer{pts,payload->if(server.sendPacket(UsbSocketServer.STREAM_TYPE_AUDIO,pts,payload))bytes.addAndGet(payload.size.toLong())}
  initialized=true
  orientationListener=object:OrientationEventListener(this,android.hardware.SensorManager.SENSOR_DELAY_UI){
   override fun onOrientationChanged(degrees:Int){
    if(degrees==ORIENTATION_UNKNOWN)return // Keep the last angle when laid flat.
    val rounded=((degrees+45)/90*90)%360
    val distance=kotlin.math.abs(degrees-rounded).let{minOf(it,360-it)}
    if(distance<=30&&rounded!=deviceOrientation){deviceOrientation=rounded;sendOrientation();syncTelemetry()}
   }
  }.also{if(it.canDetectOrientation())it.enable()}
  server.start()
  scope.launch{
   runCatching{synchronized(this@WebcamForegroundService){if(wantStreaming&&!stopping)startDefault()}}.onFailure{e->_telemetry.update{it.copy(error=e.message)}}
   while(isActive){
    val now=SystemClock.elapsedRealtime()
    synchronized(this@WebcamForegroundService){if(wantStreaming&&now-lastRecovery>4000&&(!camera.isStreaming.get()||now-camera.lastFrameMs>2500)){
     lastRecovery=now
     Log.w("UWebcam","Capture stalled; reopening selected camera "+camera.currentCameraId)
     runCatching{camera.stopStream();configure(camera.currentCameraId,camera.currentWidth,camera.currentHeight,camera.currentFps,camera.currentBitrate,camera.currentTransport)}
      .onFailure{e->_telemetry.update{it.copy(error=e.message)}}
    }}
    syncTelemetry();if(!camera.isStreaming.get()&&_telemetry.value.isClientConnected)server.sendCapabilitiesManifest(manifest.toJsonString());delay(500)
   }
  }
 }
 private fun syncTelemetry(){
  _telemetry.update{it.copy(transport=camera.currentTransport,autoOrientation=autoOrientation,rotationDegrees=rotation(),isStreaming=camera.isStreaming.get(),activeCameraId=camera.currentCameraId,width=camera.currentWidth,height=camera.currentHeight,
   fps=camera.currentFps,bitrateMbps=camera.currentBitrate/1_000_000f,bytesTransmitted=bytes.get(),isAudioActive=mic.isRunning.get(),
   torchEnabled=camera.isTorchOn,zoomRatio=camera.currentZoomRatio,exposureCompensation=camera.currentExposureComp,error=camera.lastError)}
 }
 private fun startDefault(){
  val lens=manifest.cameras.firstOrNull{it.lensFacing=="BACK"}?:manifest.cameras.firstOrNull()?:error("No available camera found")
  val mode=lens.modes.firstOrNull{it.width==1920&&it.fps==30}?:lens.modes.firstOrNull{it.width==1280&&it.fps==30}?:lens.modes.firstOrNull()?:error("No supported video mode")
  configure(lens.id,mode.width,mode.height,mode.fps,15_000_000.coerceAtMost(manifest.maxSupportedBitrate))
 }
 @Synchronized fun configure(id:String,w:Int,h:Int,fps:Int,bitrate:Int,transport:String=camera.currentTransport){
  val lens=manifest.cameras.find{it.id==id}?:error("Camera is unavailable")
  require(lens.modes.any{it.width==w&&it.height==h&&it.fps==fps}){"This camera does not support "+w+"x"+h+" at "+fps+" fps"}
  require(transport in listOf("raw","jpeg","h264")){"Unknown capture mode"}
  require(transport!="raw"||w<=640){"Minimum-delay uncompressed mode uses 480p"}
  wantStreaming=true
  promote(false)
  wakeLock?.let{if(!it.isHeld)it.acquire(12*60*60*1000L)}
  val rate=bitrate.coerceIn(1_000_000,manifest.maxSupportedBitrate)
  check(camera.startCameraStream(id,w,h,fps,rate,transport)){camera.lastError?:"Camera could not start"}
  sendOrientation()
  syncTelemetry()
 }
 fun switchCamera(id:String){val lens=manifest.cameras.find{it.id==id}?:return;val m=lens.modes.firstOrNull{it.width==camera.currentWidth&&it.fps==camera.currentFps}?:lens.modes.firstOrNull()?:return;configure(id,m.width,m.height,m.fps,15_000_000)}
 fun setProfile(profile:String){
  val lens=manifest.cameras.find{it.id==camera.currentCameraId}?:error("Camera unavailable")
  require(profile in listOf("minimum","1080")){"Unknown stream profile"}
  val target=if(profile=="minimum"){if(lens.modes.any{it.width==320&&it.fps==30})320 else 640} else 1920
  val mode=lens.modes.firstOrNull{it.width==target&&it.fps==30}?:error("This camera does not support that profile")
  configure(lens.id,mode.width,mode.height,mode.fps,15_000_000,if(profile=="minimum")"jpeg" else "h264")
 }
 fun setResolution(w:Int,h:Int,fps:Int){configure(camera.currentCameraId,w,h,fps,camera.currentBitrate)}
 fun setZoom(ratio:Float){val lens=manifest.cameras.find{it.id==camera.currentCameraId}?:return;require(ratio.isFinite()&&ratio>=lens.zoomRange.min&&ratio<=lens.zoomRange.max){"Zoom is outside the camera range"};camera.setZoom(ratio);syncTelemetry()}
 fun setExposure(value:Int){camera.setExposureCompensation(value);syncTelemetry()}
 fun setTorch(enabled:Boolean){val lens=manifest.cameras.find{it.id==camera.currentCameraId};require(!enabled||lens?.hasFlash==true){"This camera has no fill light"};camera.setTorch(enabled);syncTelemetry()}
 fun getManifest()=manifest
 @Synchronized fun pause(){wantStreaming=false;_preview.value=null;camera.stopStream();mic.stop();wakeLock?.let{if(it.isHeld)it.release()};stopForeground(STOP_FOREGROUND_REMOVE);syncTelemetry()}
 @Synchronized fun resume(){wantStreaming=true;if(!camera.isStreaming.get())configure(camera.currentCameraId,camera.currentWidth,camera.currentHeight,camera.currentFps,camera.currentBitrate)}
 private fun handle(cmd:JSONObject):JSONObject=try{
  when(cmd.optString("cmd")){
   "GET_STATE"->syncTelemetry()
   "REQUEST_KEYFRAME"->camera.requestKeyFrame()
   "START_STREAM"->resume()
   "CONFIGURE"->configure(cmd.getString("cameraId"),cmd.getInt("width"),cmd.getInt("height"),cmd.getInt("fps"),cmd.getInt("bitrate"),cmd.optString("transport",if(cmd.getInt("width")<=640)"jpeg" else "h264"))
   "SET_PROFILE"->setProfile(cmd.getString("profile"))
   "SET_AUTO_ORIENTATION"->setAutoOrientation(cmd.getBoolean("enabled"))
   "SET_ZOOM"->setZoom(cmd.getDouble("ratio").toFloat())
   "SET_EXPOSURE"->setExposure(cmd.getInt("compensation"))
   "SET_TORCH"->setTorch(cmd.getBoolean("enabled"))
   "SET_AUDIO"->{val enabled=cmd.getBoolean("enabled");if(enabled){
    check(ContextCompat.checkSelfPermission(this,Manifest.permission.RECORD_AUDIO)==PackageManager.PERMISSION_GRANTED){"Allow microphone access in the UWebcam app first"}
    promote(true);check(mic.start()){"Microphone could not start"}
   }else{mic.stop();promote(false)};syncTelemetry()}
   "STOP_STREAM"->pause()
   else->error("Unknown camera command")
  }
  JSONObject().put("status","OK").put("cmd",cmd.optString("cmd")).put("telemetry",_telemetry.value.json()).put("clockUs",SystemClock.elapsedRealtimeNanos()/1000)
 }catch(e:Exception){Log.e("UWebcam","Command failed",e);JSONObject().put("status","ERROR").put("error",e.message)}
 override fun onStartCommand(intent:Intent?,flags:Int,startId:Int):Int{
  if(intent?.action==ACTION_STOP){stopSelf();return START_NOT_STICKY}
  promote(false)
  return START_NOT_STICKY
 }
 override fun onTaskRemoved(rootIntent:Intent?){wantStreaming=false;stopSelf();super.onTaskRemoved(rootIntent)}
 private fun promote(audio:Boolean){
  val pending=PendingIntent.getActivity(this,0,Intent(this,MainActivity::class.java),PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
  val notification=NotificationCompat.Builder(this,CHANNEL).setContentTitle("UWebcam").setContentText("Your camera is available over USB").setSmallIcon(com.usbwebcam.app.R.drawable.ic_stat_webcam).setContentIntent(pending).setOngoing(true).build()
  if(Build.VERSION.SDK_INT>=30)startForeground(27183,notification,ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA or (if(audio)ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0))
  else startForeground(27183,notification)
 }
 override fun onDestroy(){
  stopping=true;wantStreaming=false
  scope.cancel()
  orientationListener?.disable()
  if(initialized){server.stop();mic.stop();camera.release()}
  wakeLock?.let{if(it.isHeld)it.release()};super.onDestroy()
 }
}
