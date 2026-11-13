import assert from "node:assert/strict";
import { candidatesForDevice } from "./brands";
const info = (productId: number, vendorId = 0x046d) => ({
  key: `${vendorId}:${productId}`, vendorId, productId,
  productString: "USB device", manufacturerString: "Logitech",
  collections: [{ usagePage: 0xff00, usage: 1, children: [], inputReports: [], outputReports: [], featureReports: [] }],
});
for (const id of [0xc359, 0xc356, 0xc357]) {
  const candidates = candidatesForDevice(info(id) as any);
  assert.equal(candidates.length, 1);
  assert.equal(candidates[0].kind, "keyboard", "Direct G915 X must use the keyboard driver");
}
assert.equal(candidatesForDevice(info(0xc08f) as any).some(c => c.kind === "keyboard"), false, "G903 must not be a keyboard");
assert.equal(candidatesForDevice(info(0xc359, 0xffff) as any).some(c => c.kind === "keyboard"), false);
console.log("Keyboard routing checks passed (G915 X, G903 and unrelated vendor).");

const { keyboardTabs, writeKeyboardLighting } = await import("./keyboard-lighting");
const { LogitechKeyboardHidClient } = await import("@openmouse/keyboard-protocol/drivers/logitech/hid");
const fixture = {logitech:{brightness:{value:50,min:0,max:100,steps:100,illumination:true,illuminationEnabled:true},lighting:{zones:[0,1],rgbZones:[{index:0,effects:[{id:0},{id:1},{id:2}]}],legacyZones:[],legacyEffectReadback:false}}};
assert.equal(keyboardTabs({}).advanced[0].controls[0].kind,"info");
assert.ok(keyboardTabs(fixture).advanced[0].controls.some(c=>c.id==="keyboard.color"));
const disjoint = {logitech:{lighting:{zones:[],legacyZones:[],rgbZones:[{effects:[{id:1}]},{effects:[{id:2}]}]}}};
assert.equal(keyboardTabs(disjoint).advanced[0].controls.some(c=>c.id==="keyboard.effect"),false,"Only effects shared by all clusters are offered");
const calls:any[]=[];
const prototype:any=LogitechKeyboardHidClient.prototype;
const originals=new Map(["open","close","readStatus","setBrightness","setSolidColor"].map(key=>[key,prototype[key]]));
try {
  prototype.open=async()=>{calls.push("open")}; prototype.close=async()=>{calls.push("close")};
  prototype.readStatus=async()=>fixture;
  prototype.setBrightness=async(value:any)=>{calls.push(["brightness",value])};
  prototype.setSolidColor=async(...value:any[])=>{calls.push(["color",...value])};
  await writeKeyboardLighting(info(0xc359) as any,"keyboard.brightness",80);
  await writeKeyboardLighting(info(0xc359) as any,"keyboard.color","#ff0000");
  assert.ok(calls.some(c=>Array.isArray(c)&&c[0]==="brightness"&&c[1]===80));
  assert.ok(calls.some(c=>Array.isArray(c)&&c.join(",")==="color,255,0,0"));
  const writes=calls.filter(Array.isArray).length;
  await assert.rejects(writeKeyboardLighting(info(0xc359) as any,"keyboard.brightness",101));
  await assert.rejects(writeKeyboardLighting(info(0xc359) as any,"keyboard.color","#garbage"));
  assert.equal(calls.filter(Array.isArray).length,writes,"Invalid values never write to the device");
  assert.equal(calls.at(-1),"close","The HID handle closes after rejection");
} finally { for (const [key,value] of originals) prototype[key]=value; }
console.log("Keyboard lighting checks passed (capabilities, writes, validation and cleanup).");
