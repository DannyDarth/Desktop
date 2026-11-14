import { decodeLogitechFirmwareInfo } from "@openmouse/keyboard-protocol/logitech";
/** Optional, read-only HID++ entity details. Uses the already resolved client. */
export async function readLogitechFirmwareDetails(client:any,keyboard:boolean) {
  const feature=await client.getFeature(0x0003);
  const index=keyboard ? feature : feature?.index;
  if (!index) return [];
  const request=async(fn:number,entity=0)=> {
    const reply=keyboard ? await client.request(index,fn,[entity,0,0]) : await client.request(index,fn<<4,entity);
    return keyboard ? reply : reply.slice(3);
  };
  const info=await request(0);
  const count=Math.min(info[0] ?? 0,8);
  const details=[];
  for(let entity=0;entity<count;entity++) {
    const decoded=decodeLogitechFirmwareInfo(await request(1,entity));
    if (decoded) details.push({component:decoded.entityType===0 ? "Device firmware" : decoded.entityType===1 ? "Bootloader" : `Entity ${decoded.entityType}`,prefix:decoded.prefix,version:decoded.version,build:decoded.build,active:decoded.active});
  }
  return details;
}
