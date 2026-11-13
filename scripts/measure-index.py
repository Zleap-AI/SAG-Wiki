#!/usr/bin/env python3
"""Measure an index_baseline binary and all of its subprocesses (macOS/Linux ps RSS).
Usage: measure-index.py BINARY PDF OUTPUT_DIR
Set SAG_BENCH_DOCLING to an installed local Docling data directory. No cloud parsing.
"""
import json, pathlib, subprocess, sys, time, platform, os, datetime

def environment():
    result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'load_average': os.getloadavg()}
    commands = {'processes': ['ps', '-axo', 'comm=']}
    if platform.system() == 'Darwin':
        commands.update(swap=['sysctl', '-n', 'vm.swapusage'], vm=['vm_stat'])
    for name, command in commands.items():
        try:
            value = subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL)
            if name == 'processes':
                result['compiler_processes'] = sum(pathlib.Path(line.strip()).name.strip('()') in
                    {'rustc', 'rust-lld', 'ld', 'clang', 'zig'} for line in value.splitlines())
            else:
                result[name] = value.strip()
        except (OSError, subprocess.CalledProcessError):
            result[name] = None
    return result
binary, pdf, output = sys.argv[1:]
out = pathlib.Path(output)
out.mkdir(parents=True, exist_ok=True)
started = time.monotonic()
peak = 0
samples = []
compiler_samples = []
direct_python = {}
system_start = environment()
with (out / 'run.log').open('w') as log:
    process = subprocess.Popen([binary, pdf, output], stdout=log, stderr=subprocess.STDOUT)
    while process.poll() is None:
        rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,rss=,comm='], text=True)
        parsed = [line.split(None, 3) for line in rows.splitlines()]
        entries = [(int(row[0]), int(row[1]), int(row[2]), row[3])
                   for row in parsed if len(row) == 4]
        descendants = {process.pid}
        for _ in range(len(entries)):
            new = {pid for pid, parent, rss, command in entries if parent in descendants}
            if new <= descendants:
                break
            descendants |= new
        for pid, parent, rss, command in entries:
            if parent == process.pid and pathlib.Path(command).name.startswith('python'):
                direct_python[pid] = command
        rss = sum(rss for pid, parent, rss, command in entries if pid in descendants)
        peak = max(peak, rss)
        elapsed = round(time.monotonic() - started, 3)
        samples.append([elapsed, rss])
        compiler_samples.append([elapsed, sum(pid not in descendants and
            pathlib.Path(command).name.strip('()') in {'rustc', 'rust-lld', 'ld', 'clang', 'zig'}
            for pid, parent, rss, command in entries)])
        time.sleep(1)
final_rows = subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True, errors='replace')
remaining_python = []
for line in final_rows.splitlines():
    fields = line.split(None, 1)
    if len(fields) == 2 and direct_python.get(int(fields[0])) == fields[1]:
        remaining_python.append(int(fields[0]))
metadata = {'platform': platform.platform(), 'machine': platform.machine(),
            'system_start': system_start, 'system_end': environment(),
            'binary': str(pathlib.Path(binary).resolve()), 'returncode': process.returncode,
            'wall_seconds': time.monotonic() - started, 'peak_process_tree_rss_kib': peak,
            'remaining_direct_python_pids': remaining_python,
            'direct_python_processes': [{'pid': pid, 'command': command} for pid, command in direct_python.items()],
            'rss_samples': samples,
            'external_compiler_samples': compiler_samples,
            'notes': '1s sampled sum of process RSS; shared pages can be double counted. Compilation excluded. Fresh index/output caches; OS/model filesystem caches uncontrolled.'}
(out / 'measurement.json').write_text(json.dumps(metadata, indent=2))
sys.exit(process.returncode)
