// Assembles an 8-byte little-endian pointer from either hex byte strings
// (as returned by the read_raw_bytes command) or already-parsed byte values.
export function parseLePtr64(bytes: (string | number)[]): bigint {
  const b = bytes.map(v => (typeof v === 'string' ? parseInt(v, 16) : v));
  const lo = BigInt(b[0]) | (BigInt(b[1]) << 8n) | (BigInt(b[2]) << 16n) | (BigInt(b[3]) << 24n);
  const hi = BigInt(b[4]) | (BigInt(b[5]) << 8n) | (BigInt(b[6]) << 16n) | (BigInt(b[7]) << 24n);
  return (hi << 32n) | lo;
}
