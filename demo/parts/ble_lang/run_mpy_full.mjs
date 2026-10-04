import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, "..", "pkg-test-handshake");
const mod = await import(path.join(pkgDir, "nrf52833_periph_wasm.js"));
await mod.default({ module_or_path: readFileSync(path.join(pkgDir, "nrf52833_periph_wasm_bg.wasm")) });
const wasm = mod;
wasm.reset_state();
const { LSM303 } = await import("../lsm303.js");
const lsm = new LSM303(wasm, "TWIM1");
wasm.qspi_register_flash("QSPI", new Array(65536).fill(0xff));
lsm.register();
wasm.init();
// Fixed sensor pose (same convention as all other probes): the default
// sine-tilt simulation returns floats that the integer regex below rejects.
lsm.auto=false; lsm.accel={x:0,y:0,z:1000}; lsm.mag={x:200,y:0,z:400};
function parseHex(text) {
  const img = new Uint8Array(512*1024).fill(0xff); const uicr=[];
  let base=0;
  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith(":")) continue;
    const n=parseInt(line.slice(1,3),16), addr=parseInt(line.slice(3,7),16), type=parseInt(line.slice(7,9),16);
    if (type===4){base=parseInt(line.slice(9,13),16)<<16;continue;}
    if (type===2){base=parseInt(line.slice(9,13),16)<<4;continue;}
    if (type!==0) continue;
    for(let i=0;i<n;i++){const a=base+addr+i,b=parseInt(line.slice(9+i*2,11+i*2),16);
      if(a>=0x10001000&&a<0x10002000)uicr.push([a,b]); else if(a<512*1024)img[a]=b;}
  }
  return {img,uicr};
}
const { img, uicr } = parseHex(readFileSync(path.join(here, "..", "..", "firmware", "micropython-microbit-v2.1.2.hex"), "utf8"));
const cpu = new wasm.WasmCpu(0x20020000, 0x20000001, 512*1024, 128*1024);
cpu.load_firmware(img, 0);
cpu.set_deliver_irqs(true);
{const words=new Map();for(const[a,b]of uicr){const w=a&~3,i=a&3;words.set(w,((words.get(w)??0xffffffff)&~(0xff<<(8*i)))|(b<<(8*i)));}for(const[w,v]of words)wasm.periph_write(w,4,v>>>0);}
const w32=(v)=>[v&0xff,(v>>8)&0xff,(v>>16)&0xff,(v>>24)&0xff];
cpu.mem_write(0x20000000, w32(0x1000));
cpu.mem_write(0x20000004, w32(0x1c000));
cpu.reset_cpu(0x20020000, 0x29c51);
let LOG="";
const uartOut=[];
const pump=()=>{
  cpu.step(20000); wasm.tick_peripherals();
  if(wasm.is_watchdog_reset_requested())cpu.reset_cpu(0x20020000,0x29c51);
  if(cpu.sleeping()){wasm.tick_n(20000);if(wasm.has_pending_interrupt())cpu.wake();}
  lsm.poll(cpu);
  let t=wasm.uarte_take_txdma();
  if(t.length)wasm.uarte_complete_txdma([...cpu.mem_read(t[0],t[1])]);
  t=wasm.nvmc_take_erase();
  if(t.length){
    if(t[0]===0xffffffff)cpu.mem_write(0,new Uint8Array(512*1024).fill(0xff));
    else cpu.mem_write(t[0],new Uint8Array(4096).fill(0xff));
    wasm.nvmc_complete_erase();
  }
  if(uartOut.length&&wasm.periph_read(0x40002108,4)===0){
    const b=uartOut.shift();
    const ptr=wasm.periph_read(0x40002534,4),amt=wasm.periph_read(0x4000253c,4),max=wasm.periph_read(0x40002538,4);
    if(ptr>=0x20000000&&amt<max&&amt<4096)cpu.mem_write(ptr+amt,new Uint8Array([b]));
    wasm.uart_rx_byte(0x40002000,b);
  }
  LOG+=wasm.get_uart_output();
};
const drip=(s,n)=>{for(const b of new TextEncoder().encode(s))uartOut.push(b);for(let i=0;i<n;i++)pump();};
let fails=0;
const check=(c,m)=>{if(!c){console.log('FAIL:',m);fails++;}else console.log('ok:',m);};
// 1. banner
for(let i=0;i<25000&&!LOG.includes('>>>');i++)pump();
check(LOG.includes('MicroPython v1.18')&&LOG.includes('>>>'),'mpy banner');
// 2. display.show(Image.HAPPY) — same CODAL display as MakeCode; matrix must light
drip('from microbit import *\r',4000);
drip('display.show(Image.HAPPY)\r',4000);
let sticky=new Array(25).fill(0);
for(let k=0;k<3000;k++){
  cpu.step(400); wasm.tick_peripherals();
  if(cpu.sleeping()){wasm.tick_n(400);if(wasm.has_pending_interrupt())cpu.wake();}
  lsm.poll(cpu);
  try{const x=wasm.uarte_take_txdma();if(x.length)wasm.uarte_complete_txdma([...cpu.mem_read(x[0],x[1])]);}catch(e){}
  LOG+=wasm.get_uart_output();
  const m=wasm.matrix_state();
  for(let j=0;j<25;j++)sticky[j]|=m[j];
}
const lit=sticky.map((v,j)=>v!==0?j:'').filter(x=>x!=='').join(',');
check(sticky.filter(v=>v!==0).length>=5,`mpy display.show matrix lit (${lit||'dark'})`);
// 3. pin0.write_digital toggles micro:bit pin0 = P0.02 (toggle 1->0->1,
// bit 2 must follow with DIR bit 2 set; absolute state, not newly-changed:
// boot/matrix leaves other P0 bits high which broke the old newly!=0 check)
drip('pin0.write_digital(1)\r',4000);
const hi1=wasm.periph_read(0x50000504,4)>>>0;
drip('pin0.write_digital(0)\r',4000);
const lo=wasm.periph_read(0x50000504,4)>>>0;
drip('pin0.write_digital(1)\r',4000);
const hi2=wasm.periph_read(0x50000504,4)>>>0;
const dir0=wasm.periph_read(0x50000514,4)>>>0;
const pin0follows=((hi1>>2)&1)===1&&((lo>>2)&1)===0&&((hi2>>2)&1)===1;
check(pin0follows&&((dir0>>2)&1)===1,`mpy pin0 toggles P0.02 (hi=0x${hi1.toString(16)} lo=0x${lo.toString(16)} dir 0x${dir0.toString(16)})`);
// 4. accelerometer readout parses as integer (sensor path end to end)
LOG='';
drip('print(accelerometer.get_x())\r',6000);
const m=LOG.match(/(-?\d+)\r\n>>> /);
check(m!==null,`mpy accel readout integer (${m?m[1]:LOG.slice(-40)})`);
check(cpu.fault_pc()===0xffffffff,'zero faults');
if(fails){console.log(`${fails} FAILURES`);process.exit(1);}
console.log('mpy full-face OK (banner+display+pins+accel)');
