// 生成 app-icon.png：透明背景 + 蓝色钥匙图形（供 `tauri icon` 生成全套图标）
const zlib = require("zlib");
const fs = require("fs");

const S = 1024;
const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}

const cx = 360, cy = 360, rOuter = 200, rInner = 110;
function isKey(x, y) {
  const dx = x - cx, dy = y - cy, d2 = dx * dx + dy * dy;
  if (d2 <= rOuter * rOuter && d2 >= rInner * rInner) return true; // 钥匙环
  if (x >= cx + rInner && x <= 900 && y >= cy - 45 && y <= cy + 45) return true; // 杆
  if (x >= 720 && x <= 780 && y >= cy + 45 && y <= cy + 150) return true; // 齿1
  if (x >= 820 && x <= 880 && y >= cy + 45 && y <= cy + 120) return true; // 齿2
  return false;
}

const raw = Buffer.alloc(S * (1 + S * 4));
let o = 0;
for (let y = 0; y < S; y++) {
  raw[o++] = 0; // filter: none
  for (let x = 0; x < S; x++) {
    if (isKey(x, y)) { raw[o++] = 0x4f; raw[o++] = 0x8c; raw[o++] = 0xff; raw[o++] = 0xff; }
    else { raw[o++] = 0; raw[o++] = 0; raw[o++] = 0; raw[o++] = 0; }
  }
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type: RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);
fs.writeFileSync("app-icon.png", png);
console.log("app-icon.png written:", png.length, "bytes");
