# A 16 x 16 RGBA PNG checked red and white in squares of 4 pixels, as the
# ball's texture in glb.rs's tests is in single pixels: each 4 x 4 block of
# the texture one color, so Basis's blocks keep it near exactly.
import struct, sys, zlib
red, white = (220, 30, 30, 255), (240, 240, 240, 255)
rows = [b'\x00' + b''.join(bytes(red if ((x // 4) + (y // 4)) % 2 == 0 else white) for x in range(16)) for y in range(16)]
chunk = lambda t, d: struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
open(sys.argv[1], 'wb').write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 16, 16, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b''.join(rows))) + chunk(b'IEND', b''))
