import {cp, mkdir, mkdtemp, rm, symlink} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const [appPath, outputPath] = process.argv.slice(2);
if (!appPath || !outputPath) throw new Error('Usage: node package-safari-dmg.mjs <app-path> <output.dmg>');
const root = path.dirname(fileURLToPath(import.meta.url));
const temporary = await mkdtemp(path.join(os.tmpdir(), 'sag-clipper-dmg-'));
try {
 const output = path.resolve(outputPath);
 await mkdir(path.dirname(output), {recursive:true});
 execFileSync('ditto', [path.resolve(appPath), path.join(temporary, 'SAG Web Clipper.app')]);
 await symlink('/Applications', path.join(temporary, 'Applications'));
 await cp(path.join(root, 'assets/dmg/finder-layout.dsstore'), path.join(temporary, '.DS_Store'));
 execFileSync('hdiutil', ['create', '-ov', '-volname', 'SAG Web Clipper', '-srcfolder', temporary, '-fs', 'HFS+', '-format', 'UDZO', '-imagekey', 'zlib-level=9', output], {stdio:'inherit'});
} finally {
 await rm(temporary, {recursive:true});
}
