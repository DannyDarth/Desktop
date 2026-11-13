import { LogitechKeyboardHidClient } from "@openmouse/keyboard-protocol/drivers/logitech/hid";
import { logitechRgbMode } from "@openmouse/keyboard-protocol/logitech";
import { TauriHidDevice, type HidInterfaceInfo } from "./tauri-hid-device";
import { withHidOpenLockRetrying } from "./hid-open-lock";
export const COLORS = [
  ["#ffffff", "White"], ["#ff0000", "Red"], ["#ff8000", "Orange"], ["#ffff00", "Yellow"],
  ["#00ff00", "Green"], ["#00ffff", "Cyan"], ["#0000ff", "Blue"], ["#8000ff", "Purple"], ["#ff00ff", "Pink"],
];
export function keyboardTabs(status: any) {
  const state = status.logitech;
  const controls: any[] = [];
  if (state?.brightness) {
    const b = state.brightness;
    controls.push({id:"keyboard.brightness",label:"Brightness",kind:"number",min:b.min,max:b.max,step:1,current:b.value});
    if (b.illumination && typeof b.illuminationEnabled === "boolean") controls.push({id:"keyboard.illumination",label:"Backlight",kind:"toggle",current:b.illuminationEnabled});
  }
  const light = state?.lighting;
  const clusters = light?.rgbZones ?? [];
  const modes = clusters.length ? [...new Set(clusters[0].effects.map((e:any) => logitechRgbMode(e.id)).filter(Boolean))].filter(mode => clusters.every((z:any) => z.effects.some((e:any) => logitechRgbMode(e.id) === mode))) : [];
  if (modes.length) controls.push({id:"keyboard.effect",label:"Lighting effect",description:"Effects are applied to all supported clusters. The keyboard does not report its active effect.",kind:"choice",options:modes.map(value=>({value,label:value})),current:null});
  const colorAvailable = light?.zones?.length || (light?.legacyEffectReadback && light.legacyZones?.some((z:any)=>z.effects.some((e:any)=>e.id===1))) || modes.includes("Static");
  if (colorAvailable) controls.push({id:"keyboard.color",label:"Static color",description:"Sets a solid color. This is a live lighting change; onboard persistence is not guaranteed.",kind:"choice",options:COLORS.map(([value,label])=>({value,label})),current:null});
  if (!controls.length) controls.push({id:"keyboard.unavailable",label:"Lighting",kind:"info",current:"This keyboard did not advertise writable lighting controls. Try a USB connection and refresh."});
  return {keyboardLighting:[{title:"Keyboard lighting",controls}], advanced:[{title:"Keyboard details",controls:[
    {id:"keyboard.model",label:"Model",kind:"info",current:status.name ?? "Logitech keyboard"},
    {id:"keyboard.firmware",label:"Firmware",kind:"info",current:status.firmwareVersion ?? "Not reported"},
    {id:"keyboard.connection",label:"Connection",kind:"info",current:status.connectionDetail ?? status.connectionType ?? "Unknown"},
    {id:"keyboard.note",label:"Support",kind:"info",current:status.ui?.statusNote ?? "Controls are limited to capabilities reported by this keyboard."},
  ]}]};
}
export async function writeKeyboardLighting(info: HidInterfaceInfo, id: string, value: any) {
  return withHidOpenLockRetrying(info.key, async () => {
    const client = new LogitechKeyboardHidClient(new TauriHidDevice(info, {usagePage:0xff00,usage:1}));
    try {
      await client.open();
      const status = await client.readStatus();
      const control = keyboardTabs(status).keyboardLighting[0].controls.find(c=>c.id===id);
      if (!control || control.kind === "info") throw new Error("This keyboard does not support that lighting control.");
      if (control.kind === "choice" && !control.options.some((o:any)=>o.value===value)) throw new Error("Unsupported lighting value.");
      if (id === "keyboard.brightness") {
        if (!Number.isInteger(value) || value < control.min || value > control.max) throw new Error("Brightness is out of range.");
        await client.setBrightness(value);
      } else if (id === "keyboard.illumination") {
        if (typeof value !== "boolean") throw new Error("Invalid backlight value.");
        await client.setIllumination(value);
      } else if (id === "keyboard.effect") await applyKeyboardEffect(client, status, value);
      else if (id === "keyboard.color") {
        const rgb = [1,3,5].map(offset=>parseInt(value.slice(offset,offset+2),16));
        const light = status.logitech!.lighting!;
        if (light.zones.length) await client.setSolidColor(rgb[0],rgb[1],rgb[2]);
        else if (light.legacyEffectReadback && light.legacyZones.some(z=>z.effects.some(e=>e.id===1))) {
          for (const zone of light.legacyZones.filter(z=>z.effects.some(e=>e.id===1))) await client.setLegacyZoneColor(zone.index,rgb[0],rgb[1],rgb[2]);
        } else await applyKeyboardEffect(client,status,"Static",rgb);
      } else throw new Error("Unknown keyboard lighting action.");
      return value;
    } finally { await client.close().catch(()=>undefined); }
  });
}

// Compatibility adapter for keyboard-protocol 1.6.1. The G915 control
// handshake is [1,3,7] (OpenRGB LogitechG915Controller::BeginModeSet), rather
// than the generic [1,1,0]. Do not swallow a rejected handshake.
export async function applyKeyboardEffect(client: LogitechKeyboardHidClient, status: any, mode: string, color = [255,255,255]) {
  if (!/^G915\b/i.test(status.name ?? "")) {
    return client.setRgbEffect(mode as any, {red:color[0],green:color[1],blue:color[2]});
  }
  const transport = client as unknown as {
    serialized<T>(work:()=>Promise<T>):Promise<T>;
    getFeature(id:number):Promise<number|null>;
    requestMode(index:number, fn:number, payload:number[], long:boolean):Promise<Uint8Array>;
  };
  return transport.serialized(async () => {
    const feature = await transport.getFeature(0x8071);
    if (!feature) throw new Error("The keyboard did not expose RGB effects.");
    const zones = status.logitech?.lighting?.rgbZones ?? [];
    const choices = zones.map((zone:any) => ({zone, effect:zone.effects.find((e:any)=>logitechRgbMode(e.id)===mode)}));
    if (!choices.length || choices.some((c:any)=>!c.effect)) throw new Error("The requested effect is not supported by every keyboard cluster.");
    await transport.requestMode(feature,5,[1,3,7],false);
    for (const {zone,effect} of choices) {
      const params = new Array(10).fill(0); const period = 5000;
      if (mode === "Static" || mode === "Breathing single") params.splice(0,3,...color);
      if (mode === "Static") params[3]=2;
      if (mode === "Cycling") { params[5]=period>>8; params[6]=period&255; params[7]=100; }
      if (mode === "Breathing single") { params[3]=period>>8; params[4]=period&255; params[6]=100; }
      if (mode === "Wave") { params[6]=period&255; params[7]=1; params[8]=100; params[9]=period>>8; }
      await transport.requestMode(feature,1,[zone.index,effect.index,...params,1,0,0,0],true);
    }
  });
}
