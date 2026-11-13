/** Confirmed user choices only. Automatic restores never overwrite newer edits. */
export class KeyboardLightingMemory {
  private nextRevision=0;
  private saved=new Map<string,{revision:number,values:Map<string,unknown>}>();
  remember(key:string,id:string,value:unknown) {
    const state=this.saved.get(key) ?? {revision:0,values:new Map<string,unknown>()};
    if(id==="keyboard.color") state.values.delete("keyboard.effect");
    if(id==="keyboard.effect") state.values.delete("keyboard.color");
    state.values.set(id,value);state.revision=++this.nextRevision;this.saved.set(key,state);
  }
  forget(key:string){this.saved.delete(key);}
  revision(key:string){return this.saved.get(key)?.revision ?? 0;}
  plan(key:string) {
    const state=this.saved.get(key);if(!state)return null;
    // Apply brightness/color first, explicit backlight Off last.
    if(state.values.get("keyboard.illumination")===false)return {revision:state.revision,commands:[{id:"keyboard.illumination",value:false}]};
    const ids=["keyboard.brightness","keyboard.color","keyboard.effect","keyboard.illumination"];
    return {revision:state.revision,commands:ids.filter(id=>state.values.has(id)).map(id=>({id,value:state.values.get(id)}))};
  }
}
