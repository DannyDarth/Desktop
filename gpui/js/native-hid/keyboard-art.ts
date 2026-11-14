/** Official model photos. Unknown boards retain the offline keyboard drawing. */
export function keyboardArt(name: string) {
  const model = name.toUpperCase().replace(/[-_]/g," ").replace(/\s+/g," ");
  if (!/\bG915\s*X\b/.test(model)) return null;
  const tkl = model.includes("TKL");
  const wireless = model.includes("LIGHTSPEED") || model.includes("WIRELESS");
  const variant = tkl ? "tkl" : wireless ? "lightspeed" : "wired";
  const source = tkl
    ? "g915-x-lightspeed-tkl/gallery/g915-x-tkl-wireless-keyboard-gallery-1-us.png"
    : wireless ? "g915-x-lightspeed-wireless/gallery/g915-x-wireless-mechanical-gaming-keyboard-gallery-1-us.png"
    : "g915-x-wired/gallery/g915-x-mechanical-gaming-keyboard-carbon-gallery-1-us.png";
  return {filename:`logitech-g915-x-${variant}-v1.png`,url:`https://resource.logitechg.com/w_1200,c_limit,q_auto,f_png,dpr_1.0/d_transparent.gif/content/dam/gaming/en/products/${source}`,art:null};
}
