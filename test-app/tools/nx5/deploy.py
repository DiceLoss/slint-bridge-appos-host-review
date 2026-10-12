#!/usr/bin/env python3
"""Archive before NX5 deployment; guard native MCP screenshots with live RAM checks."""
import argparse
import base64
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import subprocess
import sys
import urllib.request
import uuid

HERE = Path(__file__).resolve().parent
MIN_AVAILABLE_KIB = 16 * 1024


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def device_path(value):
    path = PurePosixPath(value)
    if (not path.is_absolute() or str(path) != value or '..' in path.parts
            or any(ord(c) < 32 for c in value) or '\\' in value):
        raise ValueError(f'not a canonical absolute device path: {value!r}')
    return value


def managed_path(value):
    device_path(value)
    if not re.fullmatch(r'/(run|tmp)/slint-bridge-[^/]+/.+', value):
        raise ValueError(f'outside managed Slint directories: {value}')
    return value


def retained(path, keep):
    return path.endswith('.lock') or any(path == p or path.startswith(p.rstrip('/') + '/') for p in keep)


def parse_manifest(text):
    files = {}
    for line in text.splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  (/.+)', line)
        if not match:
            raise ValueError(f'invalid inventory line: {line!r}')
        sha, path = match.groups()
        managed_path(path)
        if path in files:
            raise ValueError(f'duplicate inventory path: {path}')
        files[path] = sha
    return files


def available_kib(text):
    match = re.search(r'^MemAvailable:\s+(\d+) kB$', text, re.M)
    if not match:
        raise ValueError('missing or invalid MemAvailable; refusing operation')
    return int(match[1])


class Adb:
    def __init__(self, serial):
        self.serial = serial

    def run(self, *args):
        return subprocess.check_output(['adb', '-s', self.serial, *args], text=True,
                                       timeout=60).replace('\r\n', '\n')

    def shell(self, *args):
        # This target's older adbd does not reliably forward the shell exit code.
        command = shlex.join(args) + '; result=$?; printf "\\n__NX5_EXIT__%s\\n" "$result"'
        output = self.run('shell', command)
        match = re.search(r'\n__NX5_EXIT__(\d+)\n?$', output)
        if not match or match[1] != '0':
            raise RuntimeError(f'device command failed: {output.rstrip()}')
        return output[:match.start()].rstrip('\n')

    def read(self, path):
        return self.shell('cat', device_path(path))

    def memory(self, needed=MIN_AVAILABLE_KIB):
        available = available_kib(self.read('/proc/meminfo'))
        if available < needed:
            raise RuntimeError(f'MemAvailable={available} KiB, need {needed} KiB; archive/clean first')
        return available


def inspect(adb):
    work = '/tmp/nx5-deploy-' + uuid.uuid4().hex
    adb.shell('mkdir', work)
    adb.run('push', str(HERE / 'deploy-control.sh'), work + '/control.sh')
    adb.run('push', str(HERE / 'deploy-device.sh'), work + '/device.sh')
    print('Run in the verified NX5 root terminal:')
    print(shlex.join(['sh', work + '/device.sh', 'inventory', work]))
    print('Then use prepare --work ' + work)


def control(adb, work, mode, *args):
    if not re.fullmatch(r'/tmp/nx5-deploy-[0-9a-f]{32}', work):
        raise ValueError('use a work directory returned by inspect')
    if mode in ('control-manifest', 'purge'):
        # This small helper fits legacy adbd's command limit and works even if
        # inspect's transfer or a previous purge left the remote scripts incomplete.
        return adb.shell('sh', '-c', (HERE / 'deploy-control.sh').read_text(), 'sh', mode, work)
    return adb.shell('sh', work + '/device.sh', mode, work, *args)


def prepare(adb, work, policy_path, archive):
    if not re.fullmatch(r'/tmp/nx5-deploy-[0-9a-f]{32}', work):
        raise ValueError('use a work directory returned by inspect')
    policy = json.loads(Path(policy_path).read_text())
    binaries = [managed_path(policy[k]) for k in ('current_binary', 'recovery_binary')]
    if binaries[0] == binaries[1]:
        raise ValueError('current and recovery must be distinct files')
    keep = binaries + [device_path(p) for p in policy['keep']]
    for path in keep:
        adb.shell('test', '-e', path)
    boot = adb.read(work + '/boot-id')
    if boot != adb.read('/proc/sys/kernel/random/boot_id'):
        raise RuntimeError('inventory belongs to another boot')
    files = parse_manifest(adb.read(work + '/files.sha256'))
    if any(binary not in files for binary in binaries):
        raise ValueError('current/recovery binaries must exist as regular inventoried files')
    active = [line for line in adb.read(work + '/protected.paths').splitlines() if line.startswith('/')]
    archive = Path(archive).resolve()
    archive.mkdir(parents=True, exist_ok=False)
    control(adb, work, 'begin')  # One attempt per inventory, including interrupted attempts.
    (archive / 'policy.json').write_text(json.dumps(policy, indent=2) + '\n')
    (archive / 'inventory.json').write_text(json.dumps(files, indent=2) + '\n')
    (archive / 'retain.paths').write_text('\n'.join(keep) + '\n')
    adb.run('push', str(archive / 'retain.paths'), work + '/retain.paths')
    keep = sorted(set(keep + [device_path(p) for p in control(adb, work, 'protect').splitlines()]))
    candidates = {path: sha for path, sha in files.items() if not retained(path, keep + active)}
    for remote, sha in candidates.items():
        local = archive / 'files' / remote.lstrip('/')
        local.parent.mkdir(parents=True, exist_ok=True)
        adb.shell('sh', '-c', '[ -f "$1" ] && [ ! -L "$1" ] && [ "$(readlink -f "$1")" = "$1" ]',
                  'sh', remote)
        adb.run('pull', remote, str(local))  # Only regular files, never device aliases.
        if digest(local) != sha:
            raise RuntimeError(f'archive checksum mismatch: {remote}; no cleanup plan issued')
    plan = dict(serial=adb.serial, work=work, boot=boot, keep=sorted(set(keep)),
                files=candidates, binaries={p: files[p] for p in binaries})
    for name, contents in {
        'delete.sha256': ''.join(f'{sha}  {path}\n' for path, sha in candidates.items()),
        'retained.sha256': ''.join(f'{files[path]}  {path}\n' for path in binaries),
    }.items():
        local = archive / name
        local.write_text(contents)
        adb.run('push', str(local), work + '/' + name)
    # Publish readiness last; incomplete transfers never authorize root cleanup.
    (archive / 'boot-id').write_text(boot + '\n')
    ready = archive / 'plan.ready'
    ready.write_text(''.join(f'{digest(archive / name)}  {work}/{name}\n' for name in
                            ('boot-id', 'retain.paths', 'retained.sha256', 'delete.sha256')))
    plan['seal'] = digest(ready)
    (archive / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    adb.run('push', str(ready), work + '/plan.ready.pending')
    control(adb, work, 'seal', plan['seal'])
    print(f'Archived and verified {len(candidates)} files in {archive}')
    print('Run in the same verified root terminal:')
    print(shlex.join(['sh', work + '/device.sh', 'cleanup', work]))
    print('Then upload with --plan ' + str(archive / 'plan.json') + ', or finish without uploading.')


def load_plan(adb, path):
    plan = json.loads(Path(path).read_text())
    if plan['serial'] != adb.serial or not re.fullmatch(r'/tmp/nx5-deploy-[0-9a-f]{32}', plan['work']):
        raise ValueError('plan serial/work directory mismatch')
    if plan['boot'] != adb.read('/proc/sys/kernel/random/boot_id'):
        raise RuntimeError('device rebooted; discard this plan')
    if adb.read(plan['work'] + '/cleaned') != plan.get('seal'):
        raise RuntimeError('root cleanup has not completed for this exact plan')
    return plan


def archive_control(adb, work, archive):
    manifest = control(adb, work, 'control-manifest')
    files = []
    for line in manifest.splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  ' + re.escape(work) + r'/([a-zA-Z0-9_.-]+)', line)
        if not match or match[2] in ('.', '..'):
            raise ValueError('invalid control archive manifest')
        files.append(match.groups())
    archive = Path(archive)
    archive.mkdir(parents=True, exist_ok=False)
    for sha, name in files:
        local = archive / name
        adb.run('pull', work + '/' + name, str(local))
        if digest(local) != sha:
            raise RuntimeError(f'control archive checksum mismatch: {name}; device files retained')
    receipt = archive / 'purge.sha256'
    receipt.write_text(manifest + '\n')
    adb.run('push', str(receipt), work + '/purge.sha256')
    control(adb, work, 'purge')


def finish(adb, plan_path):
    plan = load_plan(adb, plan_path)
    archive = Path(plan_path).resolve().parent
    archive_control(adb, plan['work'], archive / ('control-' + uuid.uuid4().hex))
    print('Deployment transaction archived; device-side control files removed.')


def discard(adb, work, archive):
    """Archive an interrupted transaction's small control directory, without applying it."""
    archive_control(adb, work, archive)
    print(f'Interrupted control files archived in {archive}; no deployment files deleted.')


def upload(adb, plan_path, source, destination):
    plan = load_plan(adb, plan_path)
    managed_path(destination)
    if retained(destination, plan['keep']):
        raise ValueError('destination is retained; deploy into a new directory')
    source = Path(source)
    sha = digest(source)
    # Account for the tmpfs payload before upload, leaving the screenshot reserve.
    adb.memory(MIN_AVAILABLE_KIB + (source.stat().st_size + 1023) // 1024)
    parent = str(PurePosixPath(destination).parent)
    adb.shell('sh', '-c', 'for path; do [ ! -L "$path" ] || exit 1; done', 'sh',
              *map(str, PurePosixPath(destination).parents))
    adb.shell('mkdir', '-p', parent)
    if adb.shell('readlink', '-f', parent) != parent:
        raise ValueError('destination parent contains a symlink')
    if adb.shell('sh', '-c', 'if [ -e "$1" ] || [ -L "$1" ]; then echo exists; fi', 'sh', destination):
        raise ValueError('destination already exists; never overwrite a deployed program')
    temporary = destination + '.upload-' + uuid.uuid4().hex
    try:
        adb.run('push', str(source), temporary)
        if adb.shell('sha256sum', temporary).split()[0] != sha:
            raise RuntimeError('uploaded binary checksum mismatch')
        adb.shell('chmod', '755', temporary)
        adb.memory()
        load_plan(adb, plan_path)
        # Keep the final boot/receipt check in the same command as publication.
        adb.shell('sh', '-c',
                  '[ "$(cat /proc/sys/kernel/random/boot_id)" = "$1" ] && '
                  '[ "$(cat "$2/cleaned")" = "$3" ] && ln -- "$4" "$5"',
                  'sh', plan['boot'], plan['work'], plan['seal'], temporary, destination)
        if adb.shell('sha256sum', destination).split()[0] != sha:
            raise RuntimeError('destination changed during upload')
    finally:
        adb.shell('sh', '-c',
                  '[ "$(cat /proc/sys/kernel/random/boot_id)" != "$1" ] || rm -f -- "$2"',
                  'sh', plan['boot'], temporary)
    (Path(plan_path).resolve().parent / 'uploaded.json').write_text(
        json.dumps(dict(destination=destination, sha256=sha), indent=2) + '\n')
    finish(adb, plan_path)
    print(f'Uploaded {destination}; current display was not stopped or replaced.')


def rpc(endpoint, method, params):
    request = urllib.request.Request(endpoint, data=json.dumps(dict(
        jsonrpc='2.0', id=1, method=method, params=params)).encode(), headers={
            'Content-Type': 'application/json', 'Accept': 'application/json, text/event-stream'})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    raw = opener.open(request, timeout=30).read().decode()
    if raw.startswith(('event:', 'data:')):
        raw = next(line[5:].strip() for line in raw.splitlines() if line.startswith('data:'))
    response = json.loads(raw)
    if 'error' in response:
        raise RuntimeError(response['error'])
    result = response['result']
    if result.get('isError'):
        raise RuntimeError(result)
    return result


def screenshot(adb, output, port):
    output = Path(output)
    if output.exists():
        raise ValueError('screenshot output already exists')
    if not output.parent.is_dir():
        raise ValueError('screenshot output directory must already exist')
    adb.memory()  # Do not contact the renderer at all when the base reserve is missing.
    adb.run('forward', f'tcp:{port}', 'tcp:9315')
    endpoint = f'http://127.0.0.1:{port}/mcp'
    rpc(endpoint, 'initialize', dict(protocolVersion='2025-06-18', capabilities={},
                                  clientInfo=dict(name='nx5-deploy', version='1')))

    def tool(name, args):
        return rpc(endpoint, 'tools/call', dict(name=name, arguments=args))

    def data(result):
        return json.loads(next(b['text'] for b in result['content'] if b['type'] == 'text'))

    windows = data(tool('list_windows', {}))['windowHandles']
    if len(windows) != 1:
        raise RuntimeError('expected one full-screen NX5 window')
    args = dict(windowHandle=windows[0])
    size = data(tool('get_window_properties', args))['size']
    reserve = max(MIN_AVAILABLE_KIB, (int(size['width']) * int(size['height']) * 8 * 3 + 1023) // 1024)
    available = adb.memory(reserve)  # Recheck immediately before the allocating request.
    result = tool('take_screenshot', dict(args, imageMimeType='image/png'))
    png = base64.b64decode(next(b['data'] for b in result['content'] if b['type'] == 'image'), validate=True)
    if not png.startswith(b'\x89PNG\r\n\x1a\n'):
        raise RuntimeError('MCP did not return PNG data')
    with output.open('xb') as target:
        target.write(png)
    print(f'Saved {output}; pre-capture MemAvailable={available} KiB, reserve={reserve} KiB')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', required=True, help='explicit ADB target, never inferred from a snapshot')
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('inspect')
    p = commands.add_parser('prepare')
    p.add_argument('--work', required=True)
    p.add_argument('--retain', type=Path, required=True)
    p.add_argument('--archive', type=Path, required=True, help='new host directory for verified archives')
    p = commands.add_parser('upload')
    p.add_argument('--plan', type=Path, required=True)
    p.add_argument('source', type=Path)
    p.add_argument('destination')
    p = commands.add_parser('finish')
    p.add_argument('--plan', type=Path, required=True)
    p = commands.add_parser('discard')
    p.add_argument('--work', required=True)
    p.add_argument('--archive', type=Path, required=True)
    p = commands.add_parser('screenshot')
    p.add_argument('output', type=Path)
    p.add_argument('--port', type=int, default=19315)
    args = parser.parse_args()
    adb = Adb(args.serial)
    if args.command == 'inspect':
        inspect(adb)
    elif args.command == 'prepare':
        prepare(adb, args.work, args.retain, args.archive)
    elif args.command == 'upload':
        upload(adb, args.plan, args.source, args.destination)
    elif args.command == 'finish':
        finish(adb, args.plan)
    elif args.command == 'discard':
        discard(adb, args.work, args.archive)
    else:
        screenshot(adb, args.output, args.port)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f'nx5-deploy: {error}', file=sys.stderr)
        sys.exit(1)
