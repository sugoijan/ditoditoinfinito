// Inspect the packet ABI and check key-packet seeks without decoding.
// node --no-warnings video/packets.mjs movie.mp4 [--wasm path] [--json]
import assert from 'node:assert/strict';
import { WASI } from 'node:wasi';
import { readFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { crc32 } from 'node:zlib';
import { parseArgs } from 'node:util';
const {values: opt, positionals} = parseArgs({allowPositionals:true, options:{
  wasm:{type:'string',default:resolve(import.meta.dirname,'../target/video/ddivideo.wasm')},
  json:{type:'boolean',default:false},
}});
assert.equal(positionals.length,1,'usage: node video/packets.mjs <movie> [--wasm path] [--json]');
const file=resolve(positionals[0]);
const wasi=new WASI({version:'preview1',preopens:{'/v':dirname(file)},args:[],env:{}});
const {instance}=await WebAssembly.instantiate(await readFile(opt.wasm),{wasi_snapshot_preview1:wasi.wasiImport});
wasi.initialize(instance);
const e=instance.exports;
assert.equal(e.ddi_demux_abi?.(),1,'requires packet ABI v1');
const copy=(p,n,cap)=>{
  assert.ok(n<=cap && (!n || p!==0),'invalid packet buffer');
  const start=p>>>0, memory=new Uint8Array(e.memory.buffer);
  assert.ok(start+n<=memory.length,'packet buffer outside memory');
  return Buffer.from(memory.slice(start,start+n));
};
const text=(p,cap)=>{
  assert.notEqual(p,0);
  const b=new Uint8Array(e.memory.buffer), start=p>>>0;
  let end=start;
  while(end<b.length && end-start<cap && b[end])end++;
  assert.ok(end<b.length && end-start<cap,'unterminated packet string');
  return Buffer.from(b.subarray(start,end)).toString('utf8');
};
const path=Buffer.from('/v/'+basename(file)+'\0'), p=e.ddi_malloc(path.length);
new Uint8Array(e.memory.buffer).set(path,p>>>0);
const h=e.ddi_demux_open(p);e.ddi_free(p);
assert.notEqual(h,0,'cannot demux movie within packet limits');
try {
  const info=JSON.parse(text(e.ddi_demux_info(h),4096));
  const config=()=>copy(e.ddi_demux_config(h),e.ddi_demux_config_size(h)>>>0,1024*1024);
  const configuration=config();
  const next=()=>{
    const rc=e.ddi_demux_next(h);
    if(rc===0)return null;
    if(rc!==1)throw new Error(text(e.ddi_demux_error(h),256));
    const b=copy(e.ddi_demux_packet_info(h),72,72);
    assert.equal(b.readUInt32LE(0),72);
    const data=copy(e.ddi_demux_data(h),b.readUInt32LE(8),8*1024*1024);
    const side=copy(e.ddi_demux_side_data(h),b.readUInt32LE(12),1024*1024);
    let at=0;
    const count=b.readUInt32LE(64); assert.ok(count<=32);
    for(let i=0;i<count;i++){
      assert.ok(at+8<=side.length);
      at+=8+side.readUInt32LE(at+4);assert.ok(at<=side.length);
    }
    assert.equal(at,side.length);
    const ticks=(at)=>{const v=b.readBigInt64LE(at);return v===-(1n<<63n)?null:String(v);};
    return {pts:ticks(16),dts:ticks(24),duration:ticks(32),position:ticks(40),
      flags:b.readUInt32LE(4),sequence:String(b.readBigUInt64LE(48)),
      epoch:b.readUInt32LE(56),configuration:b.readUInt32LE(60),
      bytes:data.length,crc:crc32(data),sideBytes:side.length,sideCrc:crc32(side)};
  };
  // Keep bounded summaries, not compressed packets or a whole-file copy.
  const packets=[];let packet;
  while((packet=next())!==null){
    assert.ok(packets.length<1_000_000,'too many packets for diagnostic');
    assert.equal(packet.sequence,String(packets.length+1));
    packets.push(packet);
  }
  assert.equal(next(),null,'EOF must be sticky');
  const hints=e.ddi_demux_index_count(h);assert.ok(hints>=0 && hints<=1_000_000);
  for(let at=0;at<hints;at+=256){
    const n=e.ddi_demux_index(h,at,256);assert.ok(n>=0 && n<=256);
    copy(e.ddi_demux_index_data(h),n*32,256*32);
  }
  assert.equal(e.ddi_demux_index(h,0,257),-1,'oversized index page must fail');
  let seeks=0;
  for(const at of [Math.floor(packets.length/2),0,packets.length-1]){
    if(at<0 || !packets[at])continue;
    const timestamp=packets[at].pts??packets[at].dts;if(timestamp===null)continue;
    assert.equal(e.ddi_demux_seek(h,BigInt(timestamp)),0);
    packet=next();assert.ok(packet && (packet.flags&1));
    assert.equal(packet.epoch,++seeks);assert.equal(packet.sequence,'1');
    assert.ok(packets.some(p=>p.pts===packet.pts && p.dts===packet.dts && p.crc===packet.crc && p.bytes===packet.bytes),'seek packet differs from sequential read');
  }
  const summary={file:basename(file),info,configurationBytes:configuration.length,
    configurationCrc:crc32(configuration),packets:packets.length,indexHints:hints,seeks};
  console.log(JSON.stringify(opt.json?{...summary,records:packets}:summary));
} finally {e.ddi_demux_close(h);}
