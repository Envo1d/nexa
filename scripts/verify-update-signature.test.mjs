import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import test from "node:test";

import { verifyUpdateSignature } from "./verify-update-signature.mjs";

function signedFeed() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");

  const bytes = Buffer.from('{"Assets":[]}\n');

  const keyId = Buffer.from("12345678");

  const keyData = publicKey
    .export({ format: "der", type: "spki" })
    .subarray(-32);

  const rawSignature = sign(
    null,
    createHash("blake2b512").update(bytes).digest(),
    privateKey,
  );

  const trusted = "timestamp:123 file:releases.win-x64.json";

  const globalSignature = sign(
    null,
    Buffer.concat([rawSignature, Buffer.from(trusted)]),
    privateKey,
  );

  const encodedKey = Buffer.from(
    `untrusted comment: test\n${Buffer.concat([Buffer.from("Ed"), keyId, keyData]).toString("base64")}\n`,
  ).toString("base64");

  const signatureText = `untrusted comment: test\n${Buffer.concat([Buffer.from("ED"), keyId, rawSignature]).toString("base64")}\ntrusted comment: ${trusted}\n${globalSignature.toString("base64")}\n`;

  return {
    bytes,
    encodedKey,
    signatureText,
    encodedSignature: Buffer.from(signatureText).toString("base64"),
  };
}

test("verifies a prehashed minisign feed including the trusted comment", () => {
  const fixture = signedFeed();

  verifyUpdateSignature(
    fixture.bytes,
    fixture.encodedSignature,
    fixture.encodedKey,
  );
});

test("rejects modified feed bytes, trusted comments and a different key", () => {
  const fixture = signedFeed();

  const modifiedSignature = Buffer.from(
    fixture.signatureText.replace("timestamp:123", "timestamp:456"),
  ).toString("base64");

  assert.throws(() =>
    verifyUpdateSignature(
      Buffer.from('{"Assets":[1]}'),
      fixture.encodedSignature,
      fixture.encodedKey,
    ),
  );
  assert.throws(() =>
    verifyUpdateSignature(fixture.bytes, modifiedSignature, fixture.encodedKey),
  );
  assert.throws(() =>
    verifyUpdateSignature(
      fixture.bytes,
      fixture.encodedSignature,
      signedFeed().encodedKey,
    ),
  );
});
