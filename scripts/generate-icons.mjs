#!/usr/bin/env node
/**
 * 生成占位应用图标（PNG + ICO），避免在 M0 引入图像工具链。
 * 正式品牌图标确定后，用设计稿替换本脚本产物，并运行 `npm run tauri icon`。
 */

import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconDir = resolve(root, "apps/desktop/src-tauri/icons");
mkdirSync(iconDir, { recursive: true });

const BG = [0x36, 0x68, 0xd8, 255];
const PAPER = [0xff, 0xff, 0xff, 255];
const LINE = [0x9c, 0xb5, 0xea, 255];

function drawIcon(size) {
  const pixels = new Uint8Array(size * size * 4);
  const set = (x, y, color) => {
    if (x < 0 || y < 0 || x >= size || y >= size) return;
    const offset = (y * size + x) * 4;
    pixels[offset] = color[0];
    pixels[offset + 1] = color[1];
    pixels[offset + 2] = color[2];
    pixels[offset + 3] = color[3];
  };

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) set(x, y, BG);
  }

  const pad = Math.round(size * 0.24);
  const paperLeft = pad;
  const paperRight = size - pad;
  const paperTop = Math.round(size * 0.18);
  const paperBottom = size - Math.round(size * 0.18);
  for (let y = paperTop; y < paperBottom; y += 1) {
    for (let x = paperLeft; x < paperRight; x += 1) set(x, y, PAPER);
  }

  const lineInset = Math.round(size * 0.08);
  const lineHeight = Math.max(1, Math.round(size * 0.035));
  for (let i = 0; i < 3; i += 1) {
    const top = Math.round(size * (0.36 + i * 0.12));
    for (let y = top; y < top + lineHeight; y += 1) {
      for (let x = paperLeft + lineInset; x < paperRight - lineInset; x += 1) {
        set(x, y, LINE);
      }
    }
  }

  return pixels;
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const typeBuffer = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])));
  return Buffer.concat([length, typeBuffer, data, crc]);
}

function encodePng(size) {
  const pixels = drawIcon(size);
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y += 1) {
    raw[y * (size * 4 + 1)] = 0;
    Buffer.from(pixels.buffer, y * size * 4, size * 4).copy(
      raw,
      y * (size * 4 + 1) + 1,
    );
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function encodeIco(png) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(1, 4);

  const entry = Buffer.alloc(16);
  entry[0] = 0; // 256px
  entry[1] = 0;
  entry[2] = 0;
  entry[3] = 0;
  entry.writeUInt16LE(1, 4);
  entry.writeUInt16LE(32, 6);
  entry.writeUInt32LE(png.length, 8);
  entry.writeUInt32LE(22, 12);

  return Buffer.concat([header, entry, png]);
}

const sizes = [
  [32, "32x32.png"],
  [128, "128x128.png"],
  [256, "128x128@2x.png"],
  [256, "icon.png"],
];

for (const [size, name] of sizes) {
  writeFileSync(resolve(iconDir, name), encodePng(size));
}

const png256 = encodePng(256);
writeFileSync(resolve(iconDir, "icon.ico"), encodeIco(png256));
writeFileSync(resolve(iconDir, "icon-source.png"), encodePng(1024));

console.log(`应用图标已生成：${iconDir}`);
