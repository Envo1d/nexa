import fs from "node:fs";
import crypto from "node:crypto";

const root = "E:/Coding/Projects/Rust/nexa-updated";
const source = `${root}/prototype/target/icon-source`;
const lock = JSON.parse(fs.readFileSync(`${root}/package-lock.json`, "utf8"));
const archive = fs.readFileSync(`${source}/lucide-vue-1.52.0.tgz`);
const integrity = `sha512-${crypto.createHash("sha512").update(archive).digest("base64")}`;
if (integrity !== lock.packages["node_modules/@lucide/vue"].integrity) {
  throw new Error("Reference icon archive integrity mismatch");
}

for (const name of ["clock", "film", "scan", "square"]) {
  const text = fs.readFileSync(`${source}/package/dist/esm/icons/${name}.mjs`, "utf8");
  const data = Function(`return (${text.match(/const __iconData = ([\s\S]*?);/)[1]});`)();
  const nodes = data.node.map(([tag, attributes]) => {
    const attrs = Object.entries(attributes).filter(([key]) => key !== "key")
      .map(([key, value]) => `${key}="${value}"`).join(" ");
    return `  <${tag} ${attrs} />`;
  }).join("\n");
  fs.writeFileSync(`${root}/prototype/assets/icons/${name}.svg`,
    `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">\n${nodes}\n</svg>\n`);
}
console.log("Extracted four original Lucide icons from the integrity-verified cached archive.");
