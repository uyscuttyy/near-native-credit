import { webcrypto } from "node:crypto";

const PKCS8_ED25519_PREFIX = Buffer.from(
  "302e020100300506032b657004220420",
  "hex",
);

export function priceMessage(price) {
  return Buffer.from(
    `native-credit:v1:price:${price.eth_usd_1e8}:${price.usdc_usd_1e8}:${price.timestamp}:${price.sequence}`,
  );
}

export async function signMessage(seed, message) {
  if (!(seed instanceof Uint8Array) || seed.length !== 32) {
    throw new Error("Ed25519 seed must be 32 bytes");
  }
  const key = await webcrypto.subtle.importKey(
    "pkcs8",
    Buffer.concat([PKCS8_ED25519_PREFIX, seed]),
    { name: "Ed25519" },
    false,
    ["sign"],
  );
  return Buffer.from(
    await webcrypto.subtle.sign("Ed25519", key, message),
  ).toString("hex");
}

export async function signaturesForConfig(configKeys, keys, price) {
  const byPublicKey = new Map(keys.map((key) => [key.publicKey, key]));
  const message = priceMessage(price);
  const signatures = [];
  for (const publicKey of configKeys) {
    const key = byPublicKey.get(publicKey);
    signatures.push(key ? await signMessage(key.seed, message) : "");
  }
  return signatures;
}

export function validSignatureCount(signatures) {
  return signatures.filter((signature) => signature.length === 128).length;
}
