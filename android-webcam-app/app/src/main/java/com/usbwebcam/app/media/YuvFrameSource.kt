package com.usbwebcam.app.media

import android.graphics.ImageFormat
import android.graphics.Rect
import android.graphics.YuvImage
import android.media.Image
import android.media.ImageReader
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.Surface
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Independent camera frames avoid the hardware H264 encoder's multi-frame delay. */
class YuvFrameSource(
 private val width:Int,private val height:Int,private val transport:String,
 private val onFrame:(Byte,Long,ByteArray)->Unit
){
 private val thread=HandlerThread("Webcam latest image",android.os.Process.THREAD_PRIORITY_DISPLAY).apply{start()}
 private var frameCount=0
 private val chromaRow=ByteArray(width*2)
 private val yuvScratch=ByteArray(width*height*3/2)
 private val nv21=ByteArray(width*height*3/2)
 private val jpegOutput=ByteArrayOutputStream(width*height/4)
 private val reader=ImageReader.newInstance(width,height,ImageFormat.YUV_420_888,2)
 val surface:Surface get()=reader.surface
 @Volatile private var running=true
 init{
  reader.setOnImageAvailableListener({ source->
   val image=runCatching{source.acquireLatestImage()}.getOrNull()?:return@setOnImageAvailableListener
   try{
    if(running){
     val started=System.nanoTime();val packed=if(transport=="raw")pack(image) else packNv21(image);val packedAt=System.nanoTime()
     val boot=SystemClock.elapsedRealtimeNanos()/1000
     val age=minOf(kotlin.math.abs(boot-image.timestamp/1000),kotlin.math.abs(System.nanoTime()/1000-image.timestamp/1000))
     // Return the camera buffer before compressing/copying into the socket packet.
     // Holding it for a full JPEG encode makes the HAL queue another frame.
     image.close()
     if(transport=="raw"){
      val payload=ByteBuffer.allocate(8+packed.size).order(ByteOrder.BIG_ENDIAN).putInt(width).putInt(height).put(packed).array()
      onFrame(7,(boot-age).coerceAtLeast(0),payload)
     }else{
      val out=jpegOutput;out.reset()
      check(YuvImage(packed,ImageFormat.NV21,width,height,null).compressToJpeg(Rect(0,0,width,height),70,out))
      onFrame(8,(boot-age).coerceAtLeast(0),out.toByteArray())
      if(++frameCount%30==0)android.util.Log.i("WebcamImages","$width x $height sourceAgeMs="+age/1000+" packMs="+(packedAt-started)/1000000+" jpegMs="+(System.nanoTime()-packedAt)/1000000)
     }
    }
   }catch(e:Exception){android.util.Log.w("WebcamImages","Discarding incomplete image",e)}finally{image.close()}
  },Handler(thread.looper))
 }
 private fun packNv21(image:Image):ByteArray{
  val y=image.planes[0];val u=image.planes[1];val v=image.planes[2]
  val ub=u.buffer;val vb=v.buffer;val u0=ub.position();val v0=vb.position()
  val vu=v.pixelStride==2&&u.pixelStride==2&&(0 until minOf(32,width/2-1)).all{vb.get(v0+2*it+1)==ub.get(u0+2*it)}
  if(vu){
   val yb=y.buffer;val y0=yb.position()
   for(row in 0 until height){yb.position(y0+row*y.rowStride);yb.get(nv21,row*width,width)}
   val n=width*height
   for(row in 0 until height/2){
    vb.position(v0+row*v.rowStride);vb.get(nv21,n+row*width,width-1)
    nv21[n+row*width+width-1]=ub.get(u0+row*u.rowStride+(width/2-1)*2)
   }
   return nv21
  }
  val packed=pack(image);val n=width*height;val chroma=n/4
  System.arraycopy(packed,0,nv21,0,n)
  for(i in 0 until chroma){nv21[n+2*i]=packed[n+chroma+i];nv21[n+2*i+1]=packed[n+i]}
  return nv21
 }
 private fun pack(image:Image):ByteArray{
  val bytes=yuvScratch;var offset=0
  for(index in 0..2){
   val plane=image.planes[index];val w=if(index==0)width else width/2;val h=if(index==0)height else height/2
   val buffer=plane.buffer;val origin=buffer.position()
   for(y in 0 until h){
    val row=origin+y*plane.rowStride
    if(plane.pixelStride==1){buffer.position(row);buffer.get(bytes,offset,w);offset+=w}
    else {val count=(w-1)*plane.pixelStride+1;buffer.position(row);buffer.get(chromaRow,0,count);for(x in 0 until w)bytes[offset++]=chromaRow[x*plane.pixelStride]}
   }
  }
  return bytes
 }
 fun close(){running=false;reader.setOnImageAvailableListener(null,null);thread.quitSafely();if(Thread.currentThread()!==thread)thread.join(500);reader.close()}
}
