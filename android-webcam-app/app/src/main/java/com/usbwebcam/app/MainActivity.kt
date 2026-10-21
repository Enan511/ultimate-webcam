package com.usbwebcam.app

import android.Manifest
import android.content.*
import android.content.pm.PackageManager
import android.os.*
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.*
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import android.graphics.BitmapFactory
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.*
import androidx.core.content.ContextCompat
import com.usbwebcam.app.camera.CameraDescriptor
import com.usbwebcam.app.service.*
import kotlinx.coroutines.*

private val Lime=Color(0xFFC6F36B)
private val Ink=Color(0xFF111411)
private val Panel=Color(0xFF1B2118)
private val Muted=Color(0xFF87967B)
private val Text=Color(0xFFECF1E5)

class MainActivity:ComponentActivity(){
 private val serviceState=mutableStateOf<WebcamForegroundService?>(null)
 private var bound=false
 private val connection=object:ServiceConnection{
  override fun onServiceConnected(name:ComponentName?,binder:IBinder?){serviceState.value=(binder as? WebcamForegroundService.LocalBinder)?.getService();bound=true}
  override fun onServiceDisconnected(name:ComponentName?){serviceState.value=null;bound=false}
 }
 override fun onCreate(savedInstanceState:Bundle?){
  super.onCreate(savedInstanceState)
  window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
  window.statusBarColor=android.graphics.Color.rgb(17,20,17);window.navigationBarColor=android.graphics.Color.rgb(17,20,17)
  setContent{
   MaterialTheme(colorScheme=darkColorScheme(primary=Lime,onPrimary=Ink,surface=Panel,background=Ink,onSurface=Text)){
    PhoneStudio(serviceState.value,onStart={startCameraService()},onDim={dim->
     val params=window.attributes;params.screenBrightness=if(dim)0.02f else WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE;window.attributes=params
    })
   }
  }
 }
 override fun onStart(){super.onStart();if(hasCamera())startCameraService()}
 override fun onStop(){serviceState.value?.setPreviewVisible(false);if(bound){unbindService(connection);bound=false;serviceState.value=null};super.onStop()}
 override fun onDestroy(){if(isFinishing)stopService(Intent(this,WebcamForegroundService::class.java));super.onDestroy()}
 private fun hasCamera()=ContextCompat.checkSelfPermission(this,Manifest.permission.CAMERA)==PackageManager.PERMISSION_GRANTED
 private fun startCameraService(){
  if(!hasCamera())return
  ContextCompat.startForegroundService(this,Intent(this,WebcamForegroundService::class.java).setAction(WebcamForegroundService.ACTION_START))
  if(!bound){bound=true;bindService(Intent(this,WebcamForegroundService::class.java),connection,0)}
 }
}
@Composable
private fun PhoneStudio(service:WebcamForegroundService?,onStart:()->Unit,onDim:(Boolean)->Unit){
 val context=androidx.compose.ui.platform.LocalContext.current
 val state=service?.telemetry?.collectAsState()?.value?:ServiceTelemetry()
 val scope=rememberCoroutineScope()
 var error by remember{mutableStateOf<String?>(null)}
 var pending by remember{mutableStateOf(false)}
 var dim by remember{mutableStateOf(false)}
 var permission by remember{mutableStateOf(ContextCompat.checkSelfPermission(context,Manifest.permission.CAMERA)==PackageManager.PERMISSION_GRANTED)}
 var micAllowed by remember{mutableStateOf(ContextCompat.checkSelfPermission(context,Manifest.permission.RECORD_AUDIO)==PackageManager.PERMISSION_GRANTED)}
 val cameraPermission=rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()){result->
  permission=result[Manifest.permission.CAMERA]==true
  if(permission)onStart() else error="Camera access is needed to use your phone as a webcam. You can allow it in Android app settings."
 }
 val micPermission=rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()){micAllowed=it;if(!it)error="Microphone stays off. You can allow access later."}
 LaunchedEffect(Unit){if(!permission){val permissions=mutableListOf(Manifest.permission.CAMERA);if(Build.VERSION.SDK_INT>=33)permissions.add(Manifest.permission.POST_NOTIFICATIONS);cameraPermission.launch(permissions.toTypedArray())}}
 LaunchedEffect(service,dim){onDim(dim);service?.setPreviewVisible(!dim)}
 DisposableEffect(service){onDispose{service?.setPreviewVisible(false)}}
 DisposableEffect(Unit){onDispose{onDim(false)}}
 fun action(block:()->Unit){if(pending)return;pending=true;scope.launch{try{withContext(Dispatchers.IO){block()}}catch(e:Exception){error=e.message?:"Please try again"}finally{pending=false}}}
 val cameras=service?.getManifest()?.cameras.orEmpty()
 val lens=cameras.find{it.id==state.activeCameraId}?:cameras.firstOrNull()
 var zoom by remember{mutableFloatStateOf(1f)}
 var exposure by remember{mutableFloatStateOf(0f)}
 LaunchedEffect(state.zoomRatio){zoom=state.zoomRatio}
 LaunchedEffect(state.exposureCompensation){exposure=state.exposureCompensation.toFloat()}
 Crossfade(targetState=dim,label="display",animationSpec=tween(350)){isDim->
  if(isDim){
   Box(Modifier.fillMaxSize().background(Color.Black).clickable{dim=false},contentAlignment=Alignment.Center){
    Column(horizontalAlignment=Alignment.CenterHorizontally){
     Icon(Icons.Default.Videocam,null,tint=Lime.copy(alpha=.2f),modifier=Modifier.size(32.dp))
     Spacer(Modifier.height(18.dp));Text(if(state.isClientConnected)"Camera is live" else "Ready over USB",color=Color(0xFF30362A),fontSize=13.sp)
     Spacer(Modifier.height(6.dp));Text("Tap to wake",color=Color(0xFF22281D),fontSize=11.sp)
    }
   }
  }else{
   Surface(Modifier.fillMaxSize(),color=Ink){
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal=24.dp).padding(top=24.dp,bottom=26.dp),verticalArrangement=Arrangement.spacedBy(20.dp)){
     Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.SpaceBetween){
      Row(verticalAlignment=Alignment.CenterVertically){
       Image(painterResource(R.drawable.app_logo),contentDescription="Ultimate Webcam",modifier=Modifier.size(38.dp))
       Spacer(Modifier.width(11.dp));Column{Text("ultimate webcam",fontSize=15.sp,fontWeight=FontWeight.SemiBold,color=Text,letterSpacing=(-.5).sp);Text("PHONE COMPANION",fontSize=7.sp,letterSpacing=1.8.sp,color=Muted,modifier=Modifier.padding(top=3.dp))}
      }
      StatusPill(state.isClientConnected,state.isStreaming)
     }
     Column(Modifier.padding(top=10.dp)){
      Eyebrow("YOUR EVERYDAY, UPGRADED")
      Spacer(Modifier.height(9.dp))
      AnimatedContent(targetState=state.isClientConnected,label="headline"){connected->
       Text(if(connected)"You're in the\npicture." else "A better camera.\nAlready yours.",fontSize=35.sp,lineHeight=39.sp,fontWeight=FontWeight.Normal,letterSpacing=(-1.4).sp,color=Text)
      }
      Spacer(Modifier.height(10.dp))
      Text(if(state.isClientConnected)"Your phone. Your best angle. Live on your PC." else "Open the desktop studio. We'll take it from here.",fontSize=11.sp,lineHeight=17.sp,color=Muted)
     }
     CardSurface{
      BroadcastPreview(service,state.isStreaming,state.isClientConnected,state.width,state.height)
      Row(Modifier.fillMaxWidth().padding(17.dp),horizontalArrangement=Arrangement.SpaceBetween){
       Metric("QUALITY",if(state.isStreaming)state.height.toString()+"p" else "—")
       Metric("FRAME RATE",if(state.isStreaming)(if(state.transport=="h264"&&state.width>=1920&&Build.HARDWARE.startsWith("mt67"))minOf(state.fps,28) else state.fps).toString()+" fps" else "—")
       Metric("TRANSFERRED",formatBytes(state.bytesTransmitted))
      }
     }
     AnimatedVisibility(visible=error!=null||state.error!=null){
      Row(Modifier.fillMaxWidth().background(Color(0xFF342B1C),RoundedCornerShape(12.dp)).padding(14.dp),verticalAlignment=Alignment.CenterVertically){
       Icon(Icons.Default.Info,null,tint=Color(0xFFE4C28D),modifier=Modifier.size(17.dp));Spacer(Modifier.width(10.dp));Text(error?:state.error?:"",fontSize=11.sp,lineHeight=17.sp,color=Color(0xFFE4C28D),modifier=Modifier.weight(1f))
       IconButton(onClick={error=null}){Icon(Icons.Default.Close,"Dismiss",modifier=Modifier.size(16.dp))}
      }
     }
     Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween,verticalAlignment=Alignment.CenterVertically){
      Text("Make it yours",fontSize=15.sp,fontWeight=FontWeight.Medium,color=Text)
      Icon(Icons.Default.Tune,null,tint=Muted,modifier=Modifier.size(18.dp))
     }
     CardSurface{
      Column(Modifier.padding(18.dp).animateContentSize(),verticalArrangement=Arrangement.spacedBy(17.dp)){
       Eyebrow("STREAM MODE")
       Row(horizontalArrangement=Arrangement.spacedBy(8.dp)){
        listOf("minimum" to "Minimum delay","1080" to "Fast 1080p").forEach{(profile,label)->
         val selected=if(profile=="minimum")state.width in 1..640 else state.transport=="h264"&&state.width==1920
         OutlinedButton(onClick={action{service?.setProfile(profile)}},enabled=!pending,modifier=Modifier.weight(1f),shape=RoundedCornerShape(10.dp),colors=ButtonDefaults.outlinedButtonColors(contentColor=if(selected)Lime else Muted,containerColor=if(selected)Lime.copy(alpha=.08f) else Color.Transparent),border=BorderStroke(1.dp,if(selected)Lime.copy(alpha=.5f) else Color(0xFF35402E)),contentPadding=PaddingValues(8.dp)){
          Column{Text(label,fontSize=10.sp);Text(if(profile=="minimum")"Lowest resolution · fastest" else "Full HD · fast frames",fontSize=7.sp,color=Muted)}
         }
        }
       }
       Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween,verticalAlignment=Alignment.CenterVertically){
        Column(Modifier.weight(1f)){Text("Automatic orientation",color=Text,fontSize=12.sp);Text("Follows your phone’s position",color=Muted,fontSize=9.sp)}
        Switch(checked=state.autoOrientation,onCheckedChange={service?.setAutoOrientation(it)},colors=SwitchDefaults.colors(checkedThumbColor=Ink,checkedTrackColor=Lime))
       }
       Eyebrow("01  /  FIND YOUR ANGLE")
       Row(horizontalArrangement=Arrangement.spacedBy(8.dp)){
        cameras.forEach{cam->
         val selected=cam.id==lens?.id
         OutlinedButton(onClick={action{service?.switchCamera(cam.id)}},enabled=!pending,shape=RoundedCornerShape(10.dp),colors=ButtonDefaults.outlinedButtonColors(containerColor=if(selected)Lime.copy(alpha=.08f) else Color.Transparent,contentColor=if(selected)Lime else Muted),border=BorderStroke(1.dp,if(selected)Lime.copy(alpha=.5f) else Color(0xFF35402E)),modifier=Modifier.weight(1f),contentPadding=PaddingValues(8.dp)){
          Icon(if(cam.lensFacing=="FRONT")Icons.Default.Face else Icons.Default.CameraAlt,null,modifier=Modifier.size(15.dp));Spacer(Modifier.width(6.dp));Text(if(cam.lensFacing=="FRONT")"Front" else "Rear",fontSize=11.sp)
         }
        }
        if(cameras.isEmpty())Text("Allow camera access to see your lenses.",color=Muted,fontSize=11.sp)
       }
       Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween,verticalAlignment=Alignment.CenterVertically){
        Text("Fill light",color=Text,fontSize=12.sp)
        Switch(checked=state.torchEnabled,onCheckedChange={action{service?.setTorch(it)}},enabled=!pending&&lens?.hasFlash==true,colors=SwitchDefaults.colors(checkedThumbColor=Ink,checkedTrackColor=Lime,uncheckedTrackColor=Color(0xFF303A28),uncheckedBorderColor=Color.Transparent))
       }
       HorizontalDivider(color=Color.White.copy(alpha=.04f))
       SliderField("Zoom",zoom.toString().take(3)+"×",zoom,lens?.zoomRange?.min?:1f,lens?.zoomRange?.max?:1f,!pending&&state.isStreaming,{zoom=it},{action{service?.setZoom(zoom)}})
       if(lens!=null&&lens.exposureRange.upper>lens.exposureRange.lower){
        SliderField("Exposure",exposure.toInt().toString(),exposure,lens.exposureRange.lower.toFloat(),lens.exposureRange.upper.toFloat(),!pending&&state.isStreaming,{exposure=it},{action{service?.setExposure(exposure.toInt())}},(lens.exposureRange.upper-lens.exposureRange.lower-1).coerceAtLeast(0))
       }
      }
     }
     Row(horizontalArrangement=Arrangement.spacedBy(10.dp)){
      Button(onClick={if(!permission)cameraPermission.launch(arrayOf(Manifest.permission.CAMERA)) else action{if(state.isStreaming)service?.pause() else service?.resume()}},enabled=!pending,modifier=Modifier.weight(1f).height(49.dp),shape=RoundedCornerShape(13.dp),colors=ButtonDefaults.buttonColors(containerColor=Lime,contentColor=Ink)){
       if(pending)CircularProgressIndicator(Modifier.size(16.dp),color=Ink,strokeWidth=2.dp) else Icon(if(state.isStreaming)Icons.Default.Pause else Icons.Default.PlayArrow,null,modifier=Modifier.size(18.dp))
       Spacer(Modifier.width(7.dp));Text(if(!permission)"Allow camera" else if(pending)"One moment…" else if(state.isStreaming)"Pause camera" else "Resume camera",fontSize=11.sp,fontWeight=FontWeight.SemiBold)
      }
      OutlinedButton(onClick={dim=true},modifier=Modifier.height(49.dp),shape=RoundedCornerShape(13.dp),border=BorderStroke(1.dp,Color(0xFF35402E)),contentPadding=PaddingValues(horizontal=17.dp)){
       Icon(Icons.Default.DarkMode,null,tint=Muted,modifier=Modifier.size(17.dp));Spacer(Modifier.width(7.dp));Text("Dim display",fontSize=11.sp,color=Text)
      }
     }
     if(!micAllowed){
      Row(Modifier.fillMaxWidth().background(Panel,RoundedCornerShape(12.dp)).padding(start=14.dp,end=6.dp,top=5.dp,bottom=5.dp),verticalAlignment=Alignment.CenterVertically){
       Icon(Icons.Default.MicOff,null,tint=Muted,modifier=Modifier.size(17.dp));Spacer(Modifier.width(10.dp));Text("Optional microphone",fontSize=11.sp,color=Muted,modifier=Modifier.weight(1f));TextButton(onClick={micPermission.launch(Manifest.permission.RECORD_AUDIO)}){Text("Allow access",color=Lime,fontSize=10.sp)}
      }
     }
     Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.Center){
      Icon(Icons.Default.Shield,null,tint=Color(0xFF64834D),modifier=Modifier.size(13.dp));Spacer(Modifier.width(6.dp));Text("A direct USB connection. No cloud.",fontSize=9.sp,color=Color(0xFF64834D))
     }
     Text(Build.MANUFACTURER+" "+Build.MODEL+"  ·  UWebcam 1.3.2",fontSize=8.sp,color=Color(0xFF43503B),modifier=Modifier.fillMaxWidth(),textAlign=androidx.compose.ui.text.style.TextAlign.Center)
    }
   }
  }
 }
}
@Composable private fun Eyebrow(text:String){Text(text,fontSize=8.sp,letterSpacing=1.6.sp,color=Muted,fontWeight=FontWeight.Medium)}
@Composable private fun CardSurface(content:@Composable ColumnScope.()->Unit){Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(Panel).border(1.dp,Color.White.copy(alpha=.05f),RoundedCornerShape(18.dp)),content=content)}
@Composable private fun Metric(label:String,value:String){Column{Text(label,fontSize=7.sp,color=Muted,letterSpacing=1.sp);Spacer(Modifier.height(6.dp));Text(value,fontSize=17.sp,color=Text,fontWeight=FontWeight.Normal,letterSpacing=(-.5).sp)}}
@Composable private fun StatusPill(connected:Boolean,streaming:Boolean){Row(Modifier.background(if(connected)Lime.copy(alpha=.09f) else Color(0xFF20281C),CircleShape).padding(horizontal=10.dp,vertical=7.dp),verticalAlignment=Alignment.CenterVertically){Box(Modifier.size(5.dp).background(if(connected)Lime else Muted,CircleShape));Spacer(Modifier.width(6.dp));Text(if(connected)"LIVE" else if(streaming)"READY" else "PAUSED",color=if(connected)Lime else Muted,fontSize=8.sp,letterSpacing=.5.sp)}}
@Composable private fun SliderField(label:String,value:String,current:Float,min:Float,max:Float,enabled:Boolean,onChange:(Float)->Unit,onCommit:()->Unit,steps:Int=0){Column{Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween){Text(label,fontSize=11.sp,color=Muted);Text(value,fontSize=11.sp,color=Text)};Slider(value=current.coerceIn(min,max.coerceAtLeast(min)),onValueChange=onChange,onValueChangeFinished=onCommit,valueRange=min..max.coerceAtLeast(min+.01f),enabled=enabled&&max>min,steps=steps,colors=SliderDefaults.colors(thumbColor=Lime,activeTrackColor=Lime,inactiveTrackColor=Color(0xFF34412C)),modifier=Modifier.height(32.dp))}}
fun formatBytes(bytes:Long):String=if(bytes>=1024L*1024*1024)String.format("%.1f GB",bytes/(1024.0*1024*1024)) else String.format("%.1f MB",bytes/(1024.0*1024))

@Composable
private fun BroadcastPreview(service:WebcamForegroundService?,streaming:Boolean,connected:Boolean,width:Int,height:Int){
 val previewBytes=service?.preview?.collectAsState()?.value
 val previewImage by produceState<ImageBitmap?>(null,previewBytes,streaming,connected){
  value=if(previewBytes!=null&&streaming&&connected)withContext(Dispatchers.Default){
   BitmapFactory.decodeByteArray(previewBytes,0,previewBytes.size)?.asImageBitmap()
  }else null
  if(value!=null){delay(1500);value=null}
 }
      Box(Modifier.fillMaxWidth().aspectRatio(16f/9f).background(Brush.radialGradient(listOf(Color(0xFF34412A),Panel))),contentAlignment=Alignment.Center){
       val liveImage=previewImage.takeIf{streaming&&connected}
       if(liveImage!=null)Image(bitmap=liveImage,contentDescription="Live broadcast with studio adjustments",contentScale=ContentScale.Fit,modifier=Modifier.fillMaxSize().background(Color.Black))
       else Column(horizontalAlignment=Alignment.CenterHorizontally){
        Icon(if(streaming)Icons.Default.Videocam else Icons.Default.Pause,null,tint=Lime,modifier=Modifier.size(28.dp))
        Spacer(Modifier.height(12.dp));Text(if(connected&&streaming)"Waiting for studio preview…" else if(streaming)"Connect the desktop studio to preview" else "Camera paused",color=Muted,fontSize=10.sp)
       }
       Row(Modifier.align(Alignment.TopStart).background(Color.Black.copy(alpha=.55f),RoundedCornerShape(bottomEnd=10.dp)).padding(14.dp),verticalAlignment=Alignment.CenterVertically){Box(Modifier.size(4.dp).background(if(streaming)Lime else Muted,CircleShape));Spacer(Modifier.width(6.dp));Text(if(connected)"LIVE BROADCAST PREVIEW" else if(streaming)"READY OVER USB" else "CAMERA PAUSED",fontSize=7.sp,letterSpacing=1.sp,color=Muted)}
       Text(if(liveImage!=null)"STUDIO OUTPUT" else if(streaming)width.toString()+" × "+height else "UWEBCAM",fontSize=8.sp,color=Muted,modifier=Modifier.align(Alignment.BottomEnd).padding(14.dp))
      }
}
