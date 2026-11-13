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
