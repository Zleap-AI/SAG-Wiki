"""Linux integration: run as root in a disposable container with the worker binary.
Usage: python3 tests/fnos-document-publish.py /path/to/sag-fnos-worker
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import time

APP, ALICE, BOB = 2000, 2001, 2002
for uid, name in [(APP, 'publish-app'), (ALICE, 'publish-alice'), (BOB, 'publish-bob')]:
    subprocess.run(['useradd', '-u', str(uid), '-U', name], check=True)


def acl(*users):
    entries = [(1, 6, 0xffffffff)]
    entries += [(2, permission, uid) for uid, permission in sorted(users)]
    entries += [(4, 0, 0xffffffff), (16, 6, 0xffffffff), (32, 0, 0xffffffff)]
    return struct.pack('<I', 2) + b''.join(struct.pack('<HHI', *entry) for entry in entries)


with tempfile.TemporaryDirectory(prefix='sag-publish-') as directory:
    base = Path(directory)
    base.chmod(0o755)
    (base / 'users').mkdir()
    (base / 'state').mkdir()
    root = base / 'documents'
    root.mkdir(mode=0o777)
    root.chmod(0o777)
    connection = base / 'worker.sock'
    worker = subprocess.Popen([sys.argv[1], str(connection), str(APP), str(base / 'state')], stderr=subprocess.PIPE)
    try:
        for _ in range(100):
            if connection.exists():
                break
            assert worker.poll() is None
            time.sleep(0.05)

        def publish(name, source=None, owner=ALICE, permissions=None, expected=None, **changes):
            target = root / name
            if source is not None:
                target.write_text(source)
                os.chown(target, owner, owner)
                target.chmod(0o600)
                if permissions is not None:
                    os.setxattr(target, 'system.posix_acl_access', permissions)
            temporary = root / ('.sag-save-' + name)
            temporary.write_text('updated')
            os.chown(temporary, APP, APP)
            temporary.chmod(0o600)
            request = dict(uid=ALICE, temporary=str(temporary), target=str(target),
                           expected_hash=hashlib.sha256(source.encode()).hexdigest() if source is not None else None)
            if expected is not None:
                request['expected_hash'] = expected
            request.update(changes)
            os.seteuid(APP)
            client = socket.socket(socket.AF_UNIX)
            try:
                client.connect(str(connection))
            finally:
                os.seteuid(0)
            with client:
                client.sendall(json.dumps(dict(document_publish=request)).encode() + b'\n')
                response = json.loads(client.makefile('rb').readline())
            return target, temporary, response

        for name, owner, permissions in [('owned.md', ALICE, acl((APP, 6))),
                                         ('shared.md', BOB, acl((APP, 6), (ALICE, 6)))]:
            target, temporary, result = publish(name, 'original', owner, permissions)
            assert result == {'ok': True}, result
            assert target.read_text() == 'updated' and not temporary.exists()
            metadata = target.stat()
            assert (metadata.st_uid, metadata.st_gid, stat.S_IMODE(metadata.st_mode)) == (owner, owner, 0o660)
            assert os.getxattr(target, 'system.posix_acl_access') == permissions
            print('PASS owner/group/mode/ACL preserved:', name)

        target, temporary, result = publish('new.md')
        assert result == {'ok': True}, result
        assert (target.stat().st_uid, target.stat().st_gid) == (ALICE, ALICE)
        assert target.read_text() == 'updated'
        assert os.getxattr(target, 'system.posix_acl_access') == acl((APP, 6))
        print('PASS new document belongs to NAS user')

        target, temporary, result = publish('new.md')
        assert 'error' in result and target.read_text() == 'updated'
        print('PASS new document never overwrites an existing name')
        link = root / 'link.md'
        link.symlink_to(root / 'owned.md')
        target, temporary, result = publish('link.md', expected='stale')
        assert 'error' in result and link.is_symlink()
        assert (root / 'owned.md').read_text() == 'updated'
        print('PASS symlink source rejected without touching its target')

        inherited = acl((APP, 6), (BOB, 6))
        # Restrict inherited named grants to read-only despite their stored rw bits.
        inherited = inherited[:-16] + struct.pack('<HHI', 16, 4, 0xffffffff) + inherited[-8:]
        os.chown(root, 0, BOB)
        root.chmod(0o2777)
        os.setxattr(root, 'system.posix_acl_default', inherited)
        target, temporary, result = publish('inherited.md')
        assert result == {'ok': True}, result
        assert os.getxattr(target, 'system.posix_acl_access') == acl((APP, 6), (BOB, 4))
        assert target.stat().st_gid == BOB
        os.removexattr(root, 'system.posix_acl_default')
        root.chmod(0o777)
        print('PASS parent default ACL retained without widening other members')

        for name, permissions, expected in [('readonly.md', acl((APP, 6), (ALICE, 4)), None),
                                             ('stale.md', acl((APP, 6), (ALICE, 6)), 'outdated')]:
            target, temporary, result = publish(name, 'original', BOB, permissions, expected)
            assert 'error' in result, result
            assert target.read_text() == 'original'
            assert target.stat().st_uid == BOB
            assert os.getxattr(target, 'system.posix_acl_access') == permissions
            print('PASS original intact after rejection:', name)

        target, temporary, result = publish('system.md', 'original', 0)
        assert 'error' in result and target.read_text() == 'original'
        print('PASS root-owned source rejected')
        target, temporary, result = publish('wrong.txt', 'original')
        assert 'error' in result and target.read_text() == 'original'
        print('PASS non-Markdown source rejected')
        target, temporary, result = publish('invalid-user.md', uid=0)
        assert 'error' in result and not target.exists()
        print('PASS root user request rejected')
    finally:
        worker.send_signal(signal.SIGTERM)
        worker.wait(timeout=10)
        if worker.returncode != 0:
            raise RuntimeError(worker.stderr.read().decode())
