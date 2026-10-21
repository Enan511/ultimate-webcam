import {useEffect,useRef,useState} from "react";
import {Camera,FlipHorizontal,RotateCw,Maximize2,Video,Usb,LoaderCircle,Play,Pause} from "lucide-react";
import {listen,UnlistenFn} from "@tauri-apps/api/event";
import {command,Status} from "../studio";
interface Props{status:Status;busy:boolean;available:boolean;onConnect:()=>void;onStop:()=>void;onMirror:()=>void;onRotate:()=>void;mirrored:boolean;onError:(e:string)=>void}
export function VideoCanvas({status,busy,available,onConnect,onStop,onMirror,onRotate,mirrored,onError}:Props){
 const canvas=useRef<HTMLCanvasElement>(null);const container=useRef<HTMLDivElement>(null);
 const [fresh,setFresh]=useState(false);
 useEffect(()=>{
  let alive=true;let sequence=0;let lastFrame=0;let reported=false;let fetching=false;let again=false;let stop:UnlistenFn|undefined;
  async function draw(){
   if(!alive)return;if(fetching){again=true;return;}fetching=true;
   do{again=false;try{
    const reply=await command<ArrayBuffer | number[]>("get_preview_frame",{sequence});
    const result=reply instanceof ArrayBuffer?reply:new Uint8Array(reply).buffer;
    if(!alive)return;
    if(result.byteLength>=24){
     const h=new DataView(result);const w=h.getUint32(0,true),height=h.getUint32(4,true);
     if(w>0 && height>0 && result.byteLength===24+w*height*4){
      const c=canvas.current;if(c){if(c.width!==w||c.height!==height){c.width=w;c.height=height;}c.getContext("2d",{alpha:false,desynchronized:true})?.putImageData(new ImageData(new Uint8ClampedArray(result,24),w,height),0,0);}
      sequence=h.getUint32(20,true);lastFrame=performance.now();setFresh(true);
     }
    }
    if(performance.now()-lastFrame>2500)setFresh(false);
   }catch(e){if(alive){setFresh(false);if(!reported){reported=true;onError("Preview: "+String(e));void command("report_preview_issue",{message:String(e)}).catch(()=>{});}}}
   }while(again&&alive);fetching=false;
  }
  const timer=window.setInterval(()=>void draw(),1000);
  void listen("preview-ready",()=>void draw()).then(unlisten=>{if(!alive)unlisten();else{stop=unlisten;void draw();}}).catch(e=>onError(String(e)));
  return()=>{alive=false;clearInterval(timer);stop?.();};
 },[]);
 const visible=fresh && status.phase==="live";
 function snapshot(){
  if(!canvas.current||!visible)return;
  canvas.current.toBlob(blob=>{if(!blob)return;const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download="Ultimate-Webcam-"+Date.now()+".png";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);},"image/png");
 }
 return <div className="preview-shell" ref={container}>
  <div className="preview-grid"/>
  <canvas ref={canvas} className={"preview-canvas "+(visible?"visible":"")} aria-label="Live phone camera preview"/>
  {!visible&&<div className="preview-empty">
    <div className={"camera-orbit "+(busy?"working":"")}><div>{busy?<LoaderCircle className="spin"/>:available?<Video/>:<Usb/>}</div></div>
    <span className="eyebrow">{busy?"MAKING THE CONNECTION":available?"YOUR STUDIO IS READY":"A BETTER CAMERA. ALREADY YOURS."}</span>
    <h2>{busy?"Bringing you into focus.":available?"Ready when you are.":"Your phone. Your best angle."}</h2>
    <p>{busy?status.message:available?"One cable. A sharper picture. Start your camera to see the difference.":"Connect your Android phone with USB debugging enabled, then open UWebcam."}</p>
    {!busy&&<button className="button primary" onClick={onConnect} disabled={!available}><Play size={16} fill="currentColor"/>Start camera</button>}
  </div>}
  <div className="preview-top"><span className={"live-pill "+(visible?"live":"")}><i/>{visible?"LIVE PREVIEW":busy?"CONNECTING":"PREVIEW"}</span>{visible&&<span className="preview-size">{status.width} × {status.height}</span>}</div>
  <div className="preview-bottom">
   <span className="preview-caption">{visible?"Looking good. Make it yours.":"A little setup. A whole new perspective."}</span>
   <div className="preview-tools">
    <button title="Mirror horizontally" aria-label="Mirror horizontally" aria-pressed={mirrored} disabled={!visible} onClick={onMirror}><FlipHorizontal size={17}/></button>
    <button title="Rotate 90 degrees" aria-label="Rotate 90 degrees" disabled={!visible} onClick={onRotate}><RotateCw size={17}/></button>
    <span/>
    <button title="Save preview snapshot" aria-label="Save preview snapshot" disabled={!visible} onClick={snapshot}><Camera size={17}/></button>
    <button title="Full screen preview" aria-label="Full screen preview" onClick={()=>{if(!document.fullscreenElement)void container.current?.requestFullscreen().catch(e=>onError(String(e)));else void document.exitFullscreen();}}><Maximize2 size={17}/></button>
   </div>
  </div>
  {visible&&<button className="pause-button" title="Pause stream" onClick={onStop}><Pause size={13}/>Pause</button>}
 </div>
}
