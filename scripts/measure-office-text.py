"""Synthetic OOXML parser/index comparison; no user files or external services."""
import argparse
import hashlib
import json
import pathlib
import platform
import re
import struct
import subprocess
import zipfile
from statistics import median
p = argparse.ArgumentParser()
p.add_argument('binary')
p.add_argument('output')
a = p.parse_args()
out = pathlib.Path(a.output).resolve()
out.mkdir(parents=True, exist_ok=True)
W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
P = 'http://schemas.openxmlformats.org/presentationml/2006/main'
A = 'http://schemas.openxmlformats.org/drawingml/2006/main'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PKG = 'http://schemas.openxmlformats.org/package/2006/relationships'

def make(path, ppt, mib):
    prefix = 'ppt' if ppt else 'word'
    main = 'ppt/presentation.xml' if ppt else 'word/document.xml'
    parts = {'_rels/.rels': f'<Relationships xmlns="{PKG}"><Relationship Id="main" Type="{R}/officeDocument" Target="{main}"/></Relationships>', '[Content_Types].xml': f"""<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/{main}" ContentType="application/vnd.openxmlformats-officedocument.{('presentationml.presentation' if ppt else 'wordprocessingml.document')}.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/></Types>"""}
    if ppt:
        parts[main] = f'<p:presentation xmlns:p="{P}" xmlns:r="{R}"><p:sldIdLst><p:sldId id="256" r:id="s1"/></p:sldIdLst></p:presentation>'
        parts['ppt/_rels/presentation.xml.rels'] = f'<Relationships xmlns="{PKG}"><Relationship Id="s1" Type="{R}/slide" Target="slides/slide1.xml"/></Relationships>'
        parts['ppt/slides/slide1.xml'] = f'<p:sld xmlns:p="{P}" xmlns:a="{A}" xmlns:r="{R}"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>officetexttoken</a:t></a:r></a:p></p:txBody></p:sp>' + ''.join((f'<p:pic><p:blipFill><a:blip r:embed="img{i}"/></p:blipFill></p:pic>' for i in range(2))) + '</p:spTree></p:cSld></p:sld>'
    else:
        parts[main] = f'<w:document xmlns:w="{W}" xmlns:a="{A}" xmlns:r="{R}"><w:body><w:p><w:r><w:t>officetexttoken</w:t></w:r></w:p>' + ''.join((f'<w:p><w:r><w:drawing><a:blip r:embed="img{i}"/></w:drawing></w:r></w:p>' for i in range(2))) + '</w:body></w:document>'
    rel = 'ppt/slides/_rels/slide1.xml.rels' if ppt else 'word/_rels/document.xml.rels'
    parts[rel] = f'<Relationships xmlns="{PKG}">' + ''.join((f'''<Relationship Id="img{i}" Type="{R}/image" Target="{('../' if ppt else '')}media/{i}.bmp"/>''' for i in range(2))) + '</Relationships>'
    # Real BMP pixel payload, deliberately highly ZIP-compressible.
    width = 1024
    height = mib * 1024 * 1024 // (width * 3)
    size = width * height * 3
    header = b'BM' + struct.pack('<IHHI', size + 54, 0, 0, 54) + struct.pack('<IiiHHIIiiII', 40, width, height, 1, 24, 0, size, 0, 0, 0, 0)
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as z:
        for (n, b) in parts.items():
            z.writestr(n, b)
        for i in range(2):
            with z.open(f'{prefix}/media/{i}.bmp', 'w') as f:
                f.write(header)
                left = size
                while left:
                    n = min(left, 65536)
                    f.write(bytes(n))
                    left -= n
metadata = {'platform': platform.platform(), 'binary_sha256': hashlib.sha256(pathlib.Path(a.binary).read_bytes()).hexdigest(), 'cpu': subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip(), 'ram_bytes': int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True)), 'method': 'Three alternating paired fresh-DB runs; Store.scan wall time; process peak RSS from macOS time -l', 'limitations': 'Synthetic compressible BMPs and trivial text; warm filesystem caches; not OCR or GUI timing; ordinary OS load uncontrolled'}
(out / 'metadata.json').write_text(json.dumps(metadata, indent=2))
results = []
for ppt in [False, True]:
    for mib in [24, 70]:
        source = out / f"{('pptx' if ppt else 'docx')}-{mib * 2}MiB.{('pptx' if ppt else 'docx')}"
        make(source, ppt, mib)
        for trial in range(3):
            for mode in ['off', 'on'] if trial % 2 == 0 else ['on', 'off']:
                dest = out / f'{source.stem}-{mode}-{trial}'
                log = out / f'{dest.name}.time.txt'
                with log.open('w') as err:
                    r = subprocess.run(['/usr/bin/time', '-l', str(pathlib.Path(a.binary).resolve()), str(source), str(dest), mode], text=True, stdout=subprocess.PIPE, stderr=err)
                assert r.returncode == 0, (r.returncode, log)
                result = json.loads(r.stdout.strip().splitlines()[-1])
                assert result['scan_error'] is None
                assert result['hits'] == (0 if mib == 70 and mode == 'off' else 1), result
                m = re.search('(\\d+)\\s+maximum resident set size', log.read_text())
                assert m
                result.update(sample=source.name, trial=trial, rss_bytes=int(m[1]), input_bytes=source.stat().st_size, sha256=hashlib.sha256(source.read_bytes()).hexdigest())
                results.append(result)
                print(json.dumps(result), flush=True)
(out / 'results.json').write_text(json.dumps(results, indent=2))
for sample in sorted({r['sample'] for r in results}):
    for mode in ['off', 'on']:
        rows = [r for r in results if r['sample'] == sample and r['mode'] == mode]
        print(sample, mode, 'median_seconds', median((r['seconds'] for r in rows)), 'median_RSS_MiB', median((r['rss_bytes'] for r in rows)) / 1048576)
