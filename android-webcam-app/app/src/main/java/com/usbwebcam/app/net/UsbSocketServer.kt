package com.usbwebcam.app.net
import android.util.Log
import org.json.JSONObject
import java.io.*
import java.net.*
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class UsbSocketServer(
 private val mediaPort:Int=27183,private val controlPort:Int=27184,private val previewPort:Int=27185,
 private val onControlCommand:(JSONObject)->JSONObject?,
 private val onClientConnected:()->Unit,private val onClientDisconnected:()->Unit,
 private val onPreview:(ByteArray)->Unit,private val onNeedsKeyFrame:()->Unit
){
 companion object{const val STREAM_TYPE_VIDEO:Byte=1;const val STREAM_TYPE_AUDIO:Byte=2;const val STREAM_TYPE_CAPABILITIES:Byte=3}
 private val running=AtomicBoolean(false)
 private var mediaServer:ServerSocket?=null;private var controlServer:ServerSocket?=null;private var previewServer:ServerSocket?=null
 @Volatile private var mediaClient:Socket?=null
 @Volatile private var controlClient:Socket?=null
 @Volatile private var previewClient:Socket?=null
 private val mediaLock=Any();private val controlLock=Any()
 private var mediaThread:Thread?=null;private var controlThread:Thread?=null;private var writerThread:Thread?=null;private var previewThread:Thread?=null
 private data class Packet(val socket:Socket,val type:Byte,val pts:Long,val bytes:ByteArray)
 private val queue=ArrayBlockingQueue<Packet>(8)
 private var awaitingKeyFrame=true
 private var flowControlled=false
 private var inFlight:Long?=null
 private var refreshAfterAck=false
 fun hasMediaClient()=mediaClient?.let{!it.isClosed}==true
 fun start(){
  if(running.getAndSet(true))return
  writerThread=Thread({writeMedia()},"UsbMediaWriter").apply{start()}
  mediaThread=Thread({acceptMedia()},"UsbMediaServer").apply{start()}
  controlThread=Thread({acceptControl()},"UsbControlServer").apply{start()}
  previewThread=Thread({acceptPreview()},"UsbPreviewServer").apply{start()}
 }
 private fun server(port:Int)=ServerSocket().apply{reuseAddress=true;bind(InetSocketAddress("127.0.0.1",port))}
 private fun acceptMedia(){
  try{
   val server=server(mediaPort);mediaServer=server
   while(running.get()){
    val socket=server.accept().apply{tcpNoDelay=true;sendBufferSize=64*1024}
    mediaClient?.runCatching{close()}
    synchronized(mediaLock){mediaClient=socket;queue.clear();awaitingKeyFrame=true;flowControlled=false;inFlight=null;refreshAfterAck=false}
    onClientConnected()
    Thread({
     try{
      val input=DataInputStream(socket.getInputStream())
      while(running.get()){
       val type=input.readUnsignedByte();val pts=input.readLong();val length=input.readInt()
       require(length==0&&(type==9||type==10)){"Invalid media acknowledgement"}
       var refresh=false
       synchronized(mediaLock){
        if(mediaClient===socket){
         if(type==10){flowControlled=true;inFlight=null}
         else if(inFlight==pts){inFlight=null;refresh=refreshAfterAck;refreshAfterAck=false}
        }
       }
       if(refresh)onNeedsKeyFrame()
      }
     }
     catch(_:Exception){}
     finally{
      var disconnected=false
      synchronized(mediaLock){if(mediaClient===socket){mediaClient=null;queue.clear();disconnected=true}}
      socket.runCatching{close()}
      if(disconnected)onClientDisconnected()
     }
    },"UsbMediaDisconnect").start()
   }
  }catch(e:Exception){if(running.get())Log.e("UsbSocketServer","Media server stopped",e)}
 }
 private fun acceptPreview(){
  try{
   val server=server(previewPort);previewServer=server
   while(running.get()){
    val socket=server.accept().apply{tcpNoDelay=true}
    previewClient?.runCatching{close()};previewClient=socket
    Thread({
     try{
      val input=DataInputStream(socket.getInputStream())
      while(running.get()){
       val type=input.readUnsignedByte();input.readLong();val size=input.readInt()
       require(type==4&&size in 1..512*1024){"Invalid studio preview"}
       val jpeg=ByteArray(size);input.readFully(jpeg);onPreview(jpeg)
      }
     }catch(_:Exception){}
     finally{socket.runCatching{close()};if(previewClient===socket){previewClient=null;onPreview(byteArrayOf())}}
    },"UsbPreviewReader").start()
   }
  }catch(e:Exception){if(running.get())Log.e("UsbSocketServer","Preview server stopped",e)}
 }
 private fun writeMedia(){
  var diagnosticFrames=0
  val header=ByteBuffer.allocate(13).order(ByteOrder.BIG_ENDIAN)
  while(running.get()){
   var packet:Packet?=null
   try{
    packet=queue.poll(250,TimeUnit.MILLISECONDS)?:continue
    if(packet.socket!==mediaClient||packet.socket.isClosed)continue
    header.clear();header.put(packet.type);header.putLong(packet.pts);header.putInt(packet.bytes.size)
    val out=packet.socket.getOutputStream()
    out.write(header.array());out.write(packet.bytes)
    if(packet.type==1.toByte()&&++diagnosticFrames%150==0)Log.i("UsbSocketServer","USB write ageMs="+(android.os.SystemClock.elapsedRealtimeNanos()/1000-packet.pts)/1000)
   }catch(_:InterruptedException){if(!running.get())break}
   catch(e:Exception){packet?.socket?.runCatching{close()};Log.w("UsbSocketServer","Media write ended: "+e.message)}
  }
 }
 private fun acceptControl(){
  try{
   val server=server(controlPort);controlServer=server
   while(running.get()){
    val socket=server.accept().apply{tcpNoDelay=true}
    controlClient?.runCatching{close()};synchronized(controlLock){controlClient=socket}
    Thread({
     try{
      val reader=socket.getInputStream().bufferedReader(Charsets.UTF_8)
      val writer=socket.getOutputStream().bufferedWriter(Charsets.UTF_8)
      while(running.get()){
       val line=reader.readLine()?:break
       if(line.isBlank())continue
       val response=try{onControlCommand(JSONObject(line))}catch(e:Exception){JSONObject().put("status","ERROR").put("error",e.message)}
       if(response!=null){writer.write(response.toString());writer.newLine();writer.flush()}
      }
     }catch(_:Exception){}
     finally{socket.runCatching{close()};synchronized(controlLock){if(controlClient===socket)controlClient=null}}
    },"UsbControlCommands").start()
   }
  }catch(e:Exception){if(running.get())Log.e("UsbSocketServer","Control server stopped",e)}
 }
 fun sendPacket(type:Byte,ptsUs:Long,payload:ByteArray,keyFrame:Boolean=false):Boolean{
  var requestKey=false
  val accepted=synchronized(mediaLock){
   val socket=mediaClient?:return false
   if(socket.isClosed)return false
   val video=type==1.toByte()||type==7.toByte()||type==8.toByte()
   if(flowControlled&&video&&inFlight!=null){
    if(type==1.toByte()){awaitingKeyFrame=true;refreshAfterAck=true}
    return false
   }
   // Independent frames replace queued images. A short scheduling pause must
   // never tear down the connection.
   if(type==7.toByte()||type==8.toByte())queue.removeAll{it.type==7.toByte()||it.type==8.toByte()||it.type==1.toByte()}
   if(type==1.toByte()&&awaitingKeyFrame&&!keyFrame)return false
   if(type==6.toByte()||type==3.toByte()||type==11.toByte())queue.removeAll{it.type==type}
   val queued=if(!queue.offer(Packet(socket,type,ptsUs,payload))){
    queue.removeAll{it.type==1.toByte()||it.type==2.toByte()||it.type==7.toByte()||it.type==8.toByte()}
    awaitingKeyFrame=true
    if(type==1.toByte()&&!keyFrame){requestKey=true;false}
    else queue.offer(Packet(socket,type,ptsUs,payload)).also{if(it&&type==1.toByte()&&keyFrame)awaitingKeyFrame=false}
   }else{
    if(type==1.toByte()&&keyFrame)awaitingKeyFrame=false
    true
   }
   if(queued&&flowControlled&&video)inFlight=ptsUs
   queued
  }
  if(requestKey)onNeedsKeyFrame()
  return accepted
 }
 fun sendCapabilitiesManifest(json:String)=sendPacket(STREAM_TYPE_CAPABILITIES,System.nanoTime()/1000,json.toByteArray(Charsets.UTF_8))
 fun stop(){
  running.set(false);mediaServer?.runCatching{close()};controlServer?.runCatching{close()};previewServer?.runCatching{close()}
  mediaClient?.runCatching{close()};controlClient?.runCatching{close()};previewClient?.runCatching{close()}
  queue.clear();writerThread?.interrupt();mediaThread?.interrupt();controlThread?.interrupt();previewThread?.interrupt()
 }
}
