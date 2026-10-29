#!/usr/bin/env node
/**
 * 校验 TypeScript / JSON Schema / Pydantic 三份契约的枚举完全一致。
 * 用法：node scripts/check-contract-parity.mjs
 *
 * 这是 M0 之前的最低成本防漂移措施；后续接入正式 codegen 后仍保留作为回归检查。
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tsPath = resolve(root, "packages/contracts/ts/domain.ts");
const jsonPath = resolve(root, "packages/contracts/schemas/domain.schema.json");
const pyPath = resolve(root, "packages/contracts/python/cers_contracts/domain.py");

const tsText = readFileSync(tsPath, "utf8");
const json = JSON.parse(readFileSync(jsonPath, "utf8"));
const pyText = readFileSync(pyPath, "utf8");

function parseTsEnums(text) {
  const out = new Map();
  const re = /export type (\w+)\s*=\s*([\s\S]*?);/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const [, name, body] = m;
    const rest = body.replace(/"[^"]*"/g, "").replace(/[\s|]/g, "");
    if (rest !== "") continue;
    out.set(
      name,
      [...body.matchAll(/"([^"]+)"/g)].map((x) => x[1]).sort(),
    );
  }
  return out;
}

function parsePythonEnums(text) {
  const out = new Map();
  const re = /^(\w+)\s*=\s*Literal\[([\s\S]*?)\]\s*$/gm;
  let m;
  while ((m = re.exec(text)) !== null) {
    const [, name, body] = m;
    out.set(
      name,
      [...body.matchAll(/"([^"]+)"/g)].map((x) => x[1]).sort(),
    );
  }
  return out;
}

const tsEnums = parseTsEnums(tsText);
const pyEnums = parsePythonEnums(pyText);
const schemaEnums = new Map(
  Object.entries(json.$defs ?? {})
    .filter(([, def]) => Array.isArray(def.enum))
    .map(([name, def]) => [name, [...def.enum].sort()]),
);

const errors = [];

function diff(name, a, b, labelA, labelB) {
  if (!a) {
    errors.push(`${name}: 缺少 ${labelB}`);
    return;
  }
  const onlyA = a.filter((v) => !b.includes(v));
  const onlyB = b.filter((v) => !a.includes(v));
  if (onlyA.length || onlyB.length) {
    errors.push(
      `${name}: ${labelA} 与 ${labelB} 不一致` +
        (onlyA.length ? `；仅 ${labelA} 有 [${onlyA.join(", ")}]` : "") +
        (onlyB.length ? `；仅 ${labelB} 有 [${onlyB.join(", ")}]` : ""),
    );
  }
}

for (const [name, values] of tsEnums) {
  const schemaValues = schemaEnums.get(name);
  const pyValues = pyEnums.get(name);
  if (!schemaValues) errors.push(`${name}: JSON Schema $defs 缺少该枚举`);
  else diff(name, values, schemaValues, "TS", "JSON");
  if (!pyValues) errors.push(`${name}: Python 缺少该 Literal 别名`);
  else diff(name, values, pyValues, "TS", "Python");
}

for (const name of schemaEnums.keys()) {
  if (!tsEnums.has(name)) errors.push(`${name}: JSON Schema 有该枚举但 TS 未定义`);
}
for (const name of pyEnums.keys()) {
  if (!tsEnums.has(name)) errors.push(`${name}: Python 有该别名但 TS 未定义`);
}

if (json["x-contract-version"] !== 1) {
  errors.push(`JSON Schema x-contract-version 应为 1，实际为 ${json["x-contract-version"]}`);
}
if (!/CONTRACT_VERSION\s*=\s*1\b/.test(tsText)) {
  errors.push("TS CONTRACT_VERSION 应为 1");
}
if (!/CONTRACT_VERSION\s*=\s*1\b/.test(pyText)) {
  errors.push("Python CONTRACT_VERSION 应为 1");
}

if (errors.length) {
  console.error("契约一致性检查失败：");
  for (const e of errors) console.error(`  - ${e}`);
  process.exit(1);
}

console.log(
  `契约一致性检查通过：${tsEnums.size} 个枚举在 TS / JSON Schema / Pydantic 三端一致（contract v1）。`,
);
