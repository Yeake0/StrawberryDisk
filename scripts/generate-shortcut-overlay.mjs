import { writeFileSync } from 'node:fs';
import { crc32, deflateSync } from 'node:zlib';

// Explorer can turn an all-zero alpha overlay into an opaque black square when its
// image lists are rebuilt. Keep alpha at 1/255 everywhere, including after scaling.
// The resulting black overlay changes a background channel by at most one level.
const size = 256;
const alpha = 1;
const header = Buffer.alloc(13);
header.writeUInt32BE(size, 0);
header.writeUInt32BE(size, 4);
header[8] = 8;
header[9] = 6; // RGBA, without interlacing.

const pixels = Buffer.alloc((1 + size * 4) * size);
for (let y = 0; y < size; y += 1) {
  for (let x = 0; x < size; x += 1) {
    pixels[y * (1 + size * 4) + 1 + x * 4 + 3] = alpha;
  }
}

function chunk(type, data) {
  const payload = Buffer.concat([Buffer.from(type), data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(payload));
  return Buffer.concat([length, payload, checksum]);
}

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk('IHDR', header),
  chunk('IDAT', deflateSync(pixels, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);
const directory = Buffer.alloc(22);
directory.writeUInt16LE(1, 2); // ICO type.
directory.writeUInt16LE(1, 4); // One image; zero width/height bytes mean 256px.
directory.writeUInt16LE(1, 10);
directory.writeUInt16LE(32, 12);
directory.writeUInt32LE(png.length, 14);
directory.writeUInt32LE(directory.length, 18);
writeFileSync(
  new URL(
    '../src-tauri/crates/strawberrydisk-platform/src/windows/shortcut_overlay/transparent-v2.ico',
    import.meta.url
  ),
  Buffer.concat([directory, png])
);
