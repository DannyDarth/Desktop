import { createHash } from 'node:crypto';
import { createReadStream, writeFileSync } from 'node:fs';
import path from 'node:path';

const [output, ...files] = process.argv.slice(2);
if (!output || files.length === 0) throw new Error('Usage: sha256-files.mjs <output> <file>...');
const entries = [];
for (const file of files) {
  const name = path.basename(file);
  if (/[\r\n\\]/.test(name)) throw new Error('Unsupported checksum filename');
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  entries.push(`${hash.digest('hex')}  ${name}`);
}
writeFileSync(output, entries.join('\n') + '\n');
