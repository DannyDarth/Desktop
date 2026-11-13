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
  return {advanced:[{title:"Keyboard lighting",controls}]};
}
export async function writeKeyboardLighting(info: HidInterfaceInfo, id: string, value: any) {
  return withHidOpenLockRetrying(info.key, async () => {
    const client = new LogitechKeyboardHidClient(new TauriHidDevice(info, {usagePage:0xff00,usage:1}));
    try {
      await client.open();
      const status = await client.readStatus();
      const control = keyboardTabs(status).advanced[0].controls.find(c=>c.id===id);
      if (!control || control.kind === "info") throw new Error("This keyboard does not support that lighting control.");
      if (control.kind === "choice" && !control.options.some((o:any)=>o.value===value)) throw new Error("Unsupported lighting value.");
      if (id === "keyboard.brightness") {
        if (!Number.isInteger(value) || value < control.min || value > control.max) throw new Error("Brightness is out of range.");
        await client.setBrightness(value);
      } else if (id === "keyboard.illumination") {
        if (typeof value !== "boolean") throw new Error("Invalid backlight value.");
        await client.setIllumination(value);
      } else if (id === "keyboard.effect") await client.setRgbEffect(value);
      else if (id === "keyboard.color") {
        const rgb = [1,3,5].map(offset=>parseInt(value.slice(offset,offset+2),16));
        const light = status.logitech!.lighting!;
        if (light.zones.length) await client.setSolidColor(rgb[0],rgb[1],rgb[2]);
        else if (light.legacyEffectReadback && light.legacyZones.some(z=>z.effects.some(e=>e.id===1))) {
          for (const zone of light.legacyZones.filter(z=>z.effects.some(e=>e.id===1))) await client.setLegacyZoneColor(zone.index,rgb[0],rgb[1],rgb[2]);
        } else await client.setRgbEffect("Static",{red:rgb[0],green:rgb[1],blue:rgb[2]});
      } else throw new Error("Unknown keyboard lighting action.");
      return value;
    } finally { await client.close().catch(()=>undefined); }
  });
}
