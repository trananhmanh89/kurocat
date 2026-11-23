import fs from 'node:fs';
import zlib from 'node:zlib';
import path from 'node:path';

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return (c ^ -1) >>> 0;
}

function makeChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const t = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([t, data])), 0);
  return Buffer.concat([len, t, data, crc]);
}

function encodePng(w, h, rgba) {
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0;
    rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  }
  const idat = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([
    sig,
    makeChunk('IHDR', ihdr),
    makeChunk('IDAT', idat),
    makeChunk('IEND', Buffer.alloc(0)),
  ]);
}

function pointInTri(px, py, ax, ay, bx, by, cx, cy) {
  const v0x = cx - ax;
  const v0y = cy - ay;
  const v1x = bx - ax;
  const v1y = by - ay;
  const v2x = px - ax;
  const v2y = py - ay;
  const dot00 = v0x * v0x + v0y * v0y;
  const dot01 = v0x * v1x + v0y * v1y;
  const dot02 = v0x * v2x + v0y * v2y;
  const dot11 = v1x * v1x + v1y * v1y;
  const dot12 = v1x * v2x + v1y * v2y;
  const inv = 1 / (dot00 * dot11 - dot01 * dot01);
  const u = (dot11 * dot02 - dot01 * dot12) * inv;
  const v = (dot00 * dot12 - dot01 * dot02) * inv;
  return u >= 0 && v >= 0 && u + v <= 1;
}

function sampleCat(nx, ny) {
  const head = (nx / 0.72) ** 2 + ((ny - 0.14) / 0.62) ** 2 <= 1;
  const leftEar = pointInTri(nx, ny, -0.62, -0.05, -0.56, -0.84, -0.08, -0.36);
  const rightEar = pointInTri(nx, ny, 0.62, -0.05, 0.56, -0.84, 0.08, -0.36);
  if (!(head || leftEar || rightEar)) return [0, 0, 0, 0];

  const le = ((nx + 0.27) / 0.2) ** 2 + ((ny - 0.12) / 0.17) ** 2 <= 1;
  const re = ((nx - 0.27) / 0.2) ** 2 + ((ny - 0.12) / 0.17) ** 2 <= 1;
  if (le || re) {
    const cx = le ? -0.27 : 0.27;
    const pupil = ((nx - cx) / 0.085) ** 2 + ((ny - 0.12) / 0.125) ** 2 <= 1;
    if (pupil) {
      const glint = ((nx - (cx - 0.03)) / 0.03) ** 2 + ((ny - 0.08) / 0.03) ** 2 <= 1;
      return glint ? [255, 255, 255, 255] : [12, 12, 14, 255];
    }
    return [200, 245, 56, 255];
  }
  return [18, 18, 22, 255];
}

function renderCatIcon(size) {
  const buf = Buffer.alloc(size * size * 4);
  const ss = 4;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let sy = 0; sy < ss; sy++) {
        for (let sx = 0; sx < ss; sx++) {
          const nx = ((x + (sx + 0.5) / ss) / size) * 2 - 1;
          const ny = ((y + (sy + 0.5) / ss) / size) * 2 - 1;
          const [sr, sg, sb, sa] = sampleCat(nx, ny);
          r += sr * sa;
          g += sg * sa;
          b += sb * sa;
          a += sa;
        }
      }
      const idx = (y * size + x) * 4;
      if (a > 0) {
        buf[idx] = Math.round(r / a);
        buf[idx + 1] = Math.round(g / a);
        buf[idx + 2] = Math.round(b / a);
        buf[idx + 3] = Math.round(a / (ss * ss));
      }
    }
  }
  return encodePng(size, size, buf);
}

function buildIco(pngBuffers) {
  const count = pngBuffers.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(count, 4);
  const entries = [];
  let offset = 6 + count * 16;
  for (const { size, buf } of pngBuffers) {
    const e = Buffer.alloc(16);
    e[0] = size >= 256 ? 0 : size;
    e[1] = size >= 256 ? 0 : size;
    e[2] = 0;
    e[3] = 0;
    e.writeUInt16LE(1, 4);
    e.writeUInt16LE(32, 6);
    e.writeUInt32LE(buf.length, 8);
    e.writeUInt32LE(offset, 12);
    entries.push(e);
    offset += buf.length;
  }
  return Buffer.concat([header, ...entries, ...pngBuffers.map((x) => x.buf)]);
}

function buildIcns(entries) {
  const chunks = entries.map(({ type, buf }) => {
    const hdr = Buffer.alloc(8);
    hdr.write(type, 0, 4, 'ascii');
    hdr.writeUInt32BE(8 + buf.length, 4);
    return Buffer.concat([hdr, buf]);
  });
  const totalLen = 8 + chunks.reduce((sum, c) => sum + c.length, 0);
  const fileHdr = Buffer.alloc(8);
  fileHdr.write('icns', 0, 4, 'ascii');
  fileHdr.writeUInt32BE(totalLen, 4);
  return Buffer.concat([fileHdr, ...chunks]);
}

const iconDir = path.join('assets', 'icons');
fs.mkdirSync(iconDir, { recursive: true });
const p16 = renderCatIcon(16);
const p32 = renderCatIcon(32);
const p64 = renderCatIcon(64);
const p128 = renderCatIcon(128);
const p256 = renderCatIcon(256);
const p512 = renderCatIcon(512);
fs.writeFileSync(path.join(iconDir, '32x32.png'), p32);
fs.writeFileSync(path.join(iconDir, '128x128.png'), p128);
fs.writeFileSync(path.join(iconDir, '128x128@2x.png'), p256);
fs.writeFileSync(path.join(iconDir, '256x256.png'), p256);
fs.writeFileSync(
  path.join(iconDir, 'icon.ico'),
  buildIco([
    { size: 32, buf: p32 },
    { size: 128, buf: p128 },
    { size: 256, buf: p256 },
  ]),
);
fs.writeFileSync(
  path.join(iconDir, 'icon.icns'),
  buildIcns([
    { type: 'icp4', buf: p16 },
    { type: 'icp5', buf: p32 },
    { type: 'icp6', buf: p64 },
    { type: 'ic07', buf: p128 },
    { type: 'ic08', buf: p256 },
    { type: 'ic09', buf: p512 },
  ]),
);
console.log('Generated 32x32.png, 128x128.png, 128x128@2x.png, 256x256.png, icon.ico, icon.icns');
