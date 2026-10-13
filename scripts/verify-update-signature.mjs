import { createHash, createPublicKey, verify } from "node:crypto";

export function verifyUpdateSignature(bytes, encodedSignature, encodedKey) {
  const keyLines = Buffer.from(encodedKey.trim(), "base64").toString("utf8").split(/\r?\n/);
  const signatureLines = Buffer.from(encodedSignature.trim(), "base64").toString("utf8").split(/\r?\n/);
  const key = Buffer.from(keyLines[1] ?? "", "base64");
  const signature = Buffer.from(signatureLines[1] ?? "", "base64");
  const globalSignature = Buffer.from(signatureLines[3] ?? "", "base64");
  const algorithm = signature.subarray(0, 2).toString("ascii");

  if (key.length !== 42 || signature.length !== 74 || globalSignature.length !== 64 ||
      !signatureLines[2]?.startsWith("trusted comment: ") || !["Ed", "ED"].includes(algorithm) ||
      !key.subarray(2, 10).equals(signature.subarray(2, 10))) {
    throw new Error("Invalid minisign signature encoding or signing key.");
  }

  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), key.subarray(10)]),
    format: "der",
    type: "spki",
  });
  const payload = algorithm === "ED" ? createHash("blake2b512").update(bytes).digest() : bytes;
  const rawSignature = signature.subarray(10);
  const globalPayload = Buffer.concat([rawSignature, Buffer.from(signatureLines[2].slice(17))]);

  if (!verify(null, payload, publicKey, rawSignature) ||
      !verify(null, globalPayload, publicKey, globalSignature)) {
    throw new Error("Update feed signature does not match Nexa's trusted public key.");
  }
}
