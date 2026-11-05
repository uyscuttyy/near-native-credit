import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { resolve } from "node:path";

const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

export function base58Decode(value) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error("invalid base58 string");
  }
  let number = 0n;
  for (const character of value) {
    const index = ALPHABET.indexOf(character);
    if (index < 0) throw new Error(`invalid base58 character: ${character}`);
    number = number * 58n + BigInt(index);
  }
  const bytes = [];
  while (number > 0n) {
    bytes.push(Number(number % 256n));
    number /= 256n;
  }
  let leading = 0;
  while (leading < value.length && value[leading] === "1") leading += 1;
  return Buffer.from([...Array(leading).fill(0), ...bytes.reverse()]);
}

export function defaultKeysPath() {
  return resolve(
    process.env.ORACLE_KEYS_FILE ??
      resolve(homedir(), ".near-credentials", "native-credit-oracle-keys.json"),
  );
}

export function loadKeys(path = defaultKeysPath()) {
  const parsed = JSON.parse(readFileSync(path, "utf8"));
  if (!Array.isArray(parsed.keys) || parsed.keys.length === 0) {
    throw new Error("oracle key file has no keys");
  }
  return parsed.keys.map((key) => {
    const seed = base58Decode(key.private_key);
    if (seed.length !== 32) {
      throw new Error(`${key.name}: expected 32-byte Ed25519 seed`);
    }
    return {
      name: key.name,
      publicKey: key.public_key,
      seed,
    };
  });
}
