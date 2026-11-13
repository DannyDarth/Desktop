/** Serial, cancellable animation: never overlap frames or retry failed devices. */
export function cycleColor(step: number): number[] {
  const segment = ((step % 96) + 96) % 96;
  const sector = Math.floor(segment / 16); const rising = Math.round((segment % 16) * 255 / 16); const falling = 255-rising;
  return [[255,rising,0],[falling,255,0],[0,255,rising],[0,falling,255],[rising,0,255],[255,0,falling]][sector];
}
export class KeyboardCycle {
  private timer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private step = 1;
  constructor(private frame:(rgb:number[])=>Promise<void>, private failed:(error:unknown)=>void,
    private schedule:typeof setTimeout=setTimeout, private cancel:typeof clearTimeout=clearTimeout) {}
  start() { if (!this.stopped && this.timer === undefined) this.timer=this.schedule(()=>{ this.timer=undefined; void this.tick(); },180); }
  stop() { this.stopped=true; if (this.timer !== undefined) this.cancel(this.timer); this.timer=undefined; }
  private async tick() {
    if (this.stopped) return;
    try { await this.frame(cycleColor(this.step++)); }
    catch(error) { if (!this.stopped) { this.stop(); this.failed(error); } return; }
    if (!this.stopped) this.start();
  }
}
