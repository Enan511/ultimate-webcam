
const net=require('node:net');
const fs=require('node:fs');
const {once}=require('node:events');
const now=()=>Number(process.hrtime.bigint()/1000n);
async function main(){
 const control=net.connect(27184,'127.0.0.1');control.setNoDelay(true);await once(control,'connect');
 let rest='',lines=[],waiting;
 control.on('data',chunk=>{rest+=chunk;let i;while((i=rest.indexOf('\n'))>=0){const l=rest.slice(0,i);rest=rest.slice(i+1);if(waiting){const w=waiting;waiting=null;w(JSON.parse(l));}else lines.push(JSON.parse(l));}});
 async function command(cmd){const reply=new Promise(resolve=>{if(lines.length)resolve(lines.shift());else waiting=resolve;});control.write(JSON.stringify(cmd)+'\n');const response=await reply;if(response.status!=='OK')throw Error(response.error);return response;}
 const profile=process.argv[2]||'1080',camera=process.argv[3]||'0';const width=profile==='240'?320:profile==='minimum'?640:1920;
 await command({cmd:'CONFIGURE',cameraId:camera,width,height:width<=640?width*3/4:1080,fps:30,bitrate:Number(process.argv[5]||15000000),transport:process.argv[4]||(profile==='minimum'?'jpeg':'h264')});
 let best={rtt:Infinity};
 for(let i=0;i<12;i++){const start=now(),r=await command({cmd:'GET_STATE'}),end=now();if(end-start<best.rtt)best={rtt:end-start,offset:r.clockUs-(start+end)/2};}
 const media=net.connect(27183,'127.0.0.1');media.setNoDelay(true);await once(media,'connect');const flow=Buffer.alloc(13);flow[0]=10;media.write(flow);
 let buffer=Buffer.alloc(0),ages=[],sizes=[],first=now();
 media.on('data',data=>{buffer=Buffer.concat([buffer,data]);while(buffer.length>=13){const size=buffer.readUInt32BE(9);if(buffer.length<13+size)return;const type=buffer[0],pts=Number(buffer.readBigUInt64BE(1));if(type===1||type===7||type===8){const ack=Buffer.alloc(13);ack[0]=9;buffer.copy(ack,1,1,9);media.write(ack);ages.push((now()-(pts-best.offset))/1000);sizes.push(size);}buffer=buffer.subarray(13+size);}});
 await new Promise(resolve=>setTimeout(resolve,10000));
 media.destroy();control.destroy();
 const steady=ages.slice(10).sort((a,b)=>a-b),q=p=>steady[Math.floor((steady.length-1)*p)];
 const report={profile,camera,mode:'camera capture timestamp to PC frame receipt (excludes decoding/display)',frames:ages.length,fps:ages.length/((now()-first)/1e6),clockUncertaintyMs:best.rtt/2000,medianMs:q(.5),p95Ms:q(.95),maxMs:q(1),averagePacketBytes:sizes.reduce((a,b)=>a+b,0)/sizes.length};
 fs.writeFileSync('artifacts/latency-receive-'+profile+'-camera'+camera+'-'+(process.argv[4]||'default')+'.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
 if(!steady.length||q(.95)>250)process.exitCode=1;
}
main().catch(e=>{console.error(e);process.exitCode=1;});
setTimeout(()=>process.exit(2),20000).unref();
