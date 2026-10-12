import hashlib
import json
import os
import signal
import shutil
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
import uuid
from unittest.mock import patch

import deploy


class ArchiveDevice:
    serial = 'test-device'
    work = '/tmp/nx5-deploy-' + 'a' * 32

    def __init__(self):
        self.files = {
            '/tmp/slint-bridge-current/display': b'current',
            '/tmp/slint-bridge-recovery/display': b'recovery',
            '/tmp/slint-bridge-shared/font.ttf': b'font',
            '/tmp/slint-bridge-old/active.log': b'live',
            '/tmp/slint-bridge-old/old binary': b'obsolete',
        }
        self.calls = []
        self.corrupt = False
        self.preparing = False
        self.policy = dict(current_binary='/tmp/slint-bridge-current/display',
                           recovery_binary='/tmp/slint-bridge-recovery/display',
                           keep=['/tmp/slint-bridge-shared'])

    def read(self, path):
        if path.endswith('boot-id') or path == '/proc/sys/kernel/random/boot_id':
            return 'same-boot'
        if path.endswith('files.sha256'):
            return ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n'
                           for name, data in self.files.items())
        if path.endswith('protected.paths'):
            return '/tmp/slint-bridge-old/active.log'
        raise AssertionError(path)

    def shell(self, *args):
        self.calls.append(args)
        if args[0] == 'readlink':
            return args[-1]
        if args[0] == 'test':
            return ''
        if args[0] == 'sh':
            if args[-2] == 'begin':
                if self.preparing:
                    raise RuntimeError('transaction already prepared')
                self.preparing = True
            if args[-2] == 'protect':
                return '\n'.join(self.policy['keep'])
            return ''
        raise AssertionError(args)

    def run(self, *args):
        self.calls.append(args)
        if args[0] == 'pull':
            Path(args[2]).write_bytes(b'corrupt' if self.corrupt else self.files[args[1]])
        elif args[0] != 'push':
            raise AssertionError(args)
        return ''


class HostTests(unittest.TestCase):
    def test_cleaned_receipt_is_bound_to_the_exact_plan(self):
        adb = deploy.Adb('test')
        plan = dict(serial='test', work=ArchiveDevice.work, boot='same-boot', seal='new-plan')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'plan.json'
            path.write_text(json.dumps(plan))
            with patch.object(adb, 'read', return_value='same-boot'):
                with self.assertRaisesRegex(RuntimeError, 'plan|cleanup'):
                    deploy.load_plan(adb, path)

    def test_corrupt_control_archive_is_not_deleted(self):
        class Device:
            def __init__(self):
                self.calls = []

            def shell(self, *args):
                self.calls.append(args)
                return f'{hashlib.sha256(b"correct").hexdigest()}  {ArchiveDevice.work}/device.sh'

            def run(self, *args):
                self.calls.append(args)
                if args[0] == 'pull':
                    target = Path(args[2])
                    target.write_text('corrupted transfer')
                return ''

        adb = Device()
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises((RuntimeError, ValueError)):
                deploy.discard(adb, ArchiveDevice.work, Path(directory) / 'archive')
            self.assertFalse(any('purge' in call for call in adb.calls))

    def test_reboot_during_upload_prevents_publication(self):
        adb = deploy.Adb('test')
        plan = dict(keep=[], work=ArchiveDevice.work, boot='old-boot', seal='seal')
        commands = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'display'
            binary.write_bytes(b'new program')
            sha = deploy.digest(binary)

            def shell(*args):
                commands.append(args)
                if args[0] == 'readlink':
                    return args[-1]
                if args[0] == 'sha256sum':
                    return sha + '  ' + args[-1]
                return ''

            with patch.object(deploy, 'load_plan', side_effect=[plan, RuntimeError('device rebooted')]), \
                    patch.object(adb, 'memory', return_value=65536), \
                    patch.object(adb, 'read', return_value='new-boot'), \
                    patch.object(adb, 'run'), patch.object(adb, 'shell', side_effect=shell):
                with self.assertRaisesRegex(RuntimeError, 'rebooted'):
                    deploy.upload(adb, root / 'plan.json', binary, '/tmp/slint-bridge-new/display')
            self.assertFalse(any(' && ln ' in str(call) for call in commands), commands)

    def test_archive_preserves_current_recovery_dependencies_and_active_log(self):
        device = ArchiveDevice()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy = root / 'retain.json'
            policy.write_text(json.dumps(device.policy))
            deploy.prepare(device, device.work, policy, root / 'archive')
            plan = json.loads((root / 'archive/plan.json').read_text())
            self.assertEqual(list(plan['files']), ['/tmp/slint-bridge-old/old binary'])
            self.assertEqual((root / 'archive/files/tmp/slint-bridge-old/old binary').read_bytes(),
                             b'obsolete')
            self.assertEqual(sum(c[0] == 'push' for c in device.calls), 4)
            self.assertEqual(plan['seal'], deploy.digest(root / 'archive/plan.ready'))
            with self.assertRaisesRegex(RuntimeError, 'already prepared'):
                deploy.prepare(device, device.work, policy, root / 'second-archive')

    def test_corrupt_archive_never_issues_cleanup_files(self):
        device = ArchiveDevice()
        device.corrupt = True
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy = root / 'retain.json'
            policy.write_text(json.dumps(device.policy))
            with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                deploy.prepare(device, device.work, policy, root / 'archive')
            self.assertFalse(any('seal' in c for c in device.calls))
            self.assertFalse(any(c[0] == 'push' and c[-1].endswith('delete.sha256')
                                 for c in device.calls))
            self.assertFalse((root / 'archive/plan.json').exists())

    def test_missing_recovery_refuses_to_plan_any_cleanup(self):
        device = ArchiveDevice()
        del device.files[device.policy['recovery_binary']]
        with tempfile.TemporaryDirectory() as directory:
            policy = Path(directory) / 'retain.json'
            policy.write_text(json.dumps(device.policy))
            with self.assertRaisesRegex(ValueError, 'current/recovery'):
                deploy.prepare(device, device.work, policy, Path(directory) / 'archive')
            self.assertFalse(any(c[0] in ('pull', 'push') for c in device.calls))

    def test_failed_discard_archive_does_not_remove_control_directory(self):
        adb = deploy.Adb('test')
        manifest = f'{"0" * 64}  {ArchiveDevice.work}/device.sh'
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(adb, 'shell', return_value=manifest) as shell, \
                patch.object(adb, 'run', side_effect=OSError('transfer failed')):
            with self.assertRaisesRegex(OSError, 'transfer failed'):
                deploy.discard(adb, ArchiveDevice.work, Path(directory) / 'archive')
            self.assertFalse(any('purge' in call.args for call in shell.call_args_list))

    def test_manifest_rejects_escape_and_multiline_paths(self):
        sha = '0' * 64
        for path in ['/etc/passwd', '/tmp/slint-bridge-old/../../etc/passwd',
                     '/tmp/slint-bridge-old/a\n/tmp/slint-bridge-old/b']:
            with self.subTest(path=path), self.assertRaises(ValueError):
                deploy.parse_manifest(f'{sha}  {path}')

    def test_adb_shell_checks_remote_status_even_when_adb_returns_zero(self):
        adb = deploy.Adb('test')
        with patch.object(adb, 'run', return_value='denied\n__NX5_EXIT__1\n'):
            with self.assertRaisesRegex(RuntimeError, 'device command failed'):
                adb.shell('false')
        with patch.object(adb, 'run', return_value='data\n__NX5_EXIT__0\n'):
            self.assertEqual(adb.shell('cat', '/test'), 'data')

    def test_low_or_missing_memory_does_not_contact_mcp(self):
        for meminfo in ['MemAvailable: 16383 kB\n', 'MemFree: 99999 kB\n']:
            adb = deploy.Adb('test')
            with patch.object(adb, 'read', return_value=meminfo), patch.object(deploy, 'rpc') as rpc:
                with self.assertRaises((RuntimeError, ValueError)):
                    deploy.screenshot(adb, '/not-created.png', 19315)
                rpc.assert_not_called()

    def test_memory_is_rechecked_after_window_query_and_scaled_to_size(self):
        def text(data):
            return dict(content=[dict(type='text', text=json.dumps(data))])

        for size, memory in [((1024, 600), 16383), ((3840, 2160), 20000)]:
            adb = deploy.Adb('test')
            responses = [{}, text(dict(windowHandles=[{}])),
                         text(dict(size=dict(width=size[0], height=size[1])))]
            with patch.object(adb, 'run'), patch.object(adb, 'read', side_effect=[
                'MemAvailable: 65536 kB\n', f'MemAvailable: {memory} kB\n'
            ]), patch.object(deploy, 'rpc', side_effect=responses) as rpc:
                with self.assertRaisesRegex(RuntimeError, 'MemAvailable'):
                    deploy.screenshot(adb, '/not-created.png', 19315)
                self.assertNotIn('take_screenshot', str(rpc.call_args_list))

    def test_low_memory_upload_keeps_old_program_and_never_pushes(self):
        adb = deploy.Adb('test')
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'display'
            binary.write_bytes(b'new')
            with patch.object(deploy, 'load_plan', return_value={'keep': []}), \
                    patch.object(adb, 'read', return_value='MemAvailable: 16384 kB\n'), \
                    patch.object(adb, 'run') as run:
                with self.assertRaisesRegex(RuntimeError, 'need 16385'):
                    deploy.upload(adb, Path(directory) / 'plan.json', binary,
                                  '/tmp/slint-bridge-new/display')
                run.assert_not_called()

class DeviceCleanupTests(unittest.TestCase):
    """Execute the actual cleanup shell, with only root/proc paths redirected."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='slint-bridge-deploy-test-')
        self.root = Path(self.directory.name)
        self.work = Path('/tmp/nx5-deploy-' + uuid.uuid4().hex)
        self.work.mkdir()
        self.proc = self.root / 'proc'
        (self.proc / 'sys/kernel/random').mkdir(parents=True)
        (self.proc / 'sys/kernel/random/boot_id').write_text('same-boot\n')
        (self.proc / 'meminfo').write_text('MemAvailable: 65536 kB\n')
        (self.work / 'boot-id').write_text('same-boot\n')
        self.current = self.root / 'current'
        self.recovery = self.root / 'recovery'
        self.old = self.root / 'old file'
        for path in (self.current, self.recovery, self.old):
            path.write_text(path.name)
        (self.work / 'retained.sha256').write_text(self.lines(self.current, self.recovery))
        (self.work / 'retain.paths').write_text(f'{self.current}\n{self.recovery}\n')
        (self.work / 'delete.sha256').write_text(self.lines(self.old))
        script = (deploy.HERE / 'deploy-device.sh').read_text()
        script = script.replace('[ "$(id -u)" = 0 ]', 'true')
        script = script.replace('/proc/', str(self.proc) + '/')
        script = script.replace('/tmp/nx5-deploy-cleanup.lock', str(self.root / 'cleanup.lock'))
        self.script = self.work / 'device.sh'
        self.script.write_text(script)
        control = (deploy.HERE / 'deploy-control.sh').read_text()
        self.control_script = control.replace('/tmp/nx5-deploy-cleanup.lock',
                                              str(self.root / 'cleanup.lock'))
        (self.work / 'control.sh').write_text(self.control_script)

    def tearDown(self):
        shutil.rmtree(self.work, ignore_errors=True)
        self.directory.cleanup()

    def lines(self, *paths):
        return ''.join(f'{deploy.digest(p)}  {p}\n' for p in paths)

    def seal(self):
        (self.work / 'plan.ready').write_text(self.lines(*[self.work / name for name in
            ('boot-id', 'retain.paths', 'retained.sha256', 'delete.sha256')]))

    def run_cleanup(self, seal=True):
        if seal:
            self.seal()
        return subprocess.run(['sh', str(self.script), 'cleanup', str(self.work)],
                              capture_output=True, text=True)

    def test_deletes_only_verified_file_and_retains_binaries(self):
        result = self.run_cleanup()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.old.exists())
        self.assertTrue(self.current.exists() and self.recovery.exists())
        self.assertEqual((self.work / 'cleaned').read_text(),
                         deploy.digest(self.work / 'plan.ready') + '\n')

    def test_partial_or_modified_plan_cannot_delete_files(self):
        self.assertNotEqual(self.run_cleanup(seal=False).returncode, 0)
        self.seal()
        (self.work / 'retain.paths').write_text('modified\n')
        self.assertNotEqual(self.run_cleanup(seal=False).returncode, 0)
        self.assertTrue(self.old.exists())

    def test_truncated_ready_manifest_cannot_authorize_unarchived_deletion(self):
        self.seal()
        ready = self.work / 'plan.ready'
        ready.write_text(ready.read_text().splitlines()[0] + '\n')
        unarchived = self.root / 'not-archived'
        unarchived.write_text('must survive')
        with (self.work / 'delete.sha256').open('a') as target:
            target.write(self.lines(unarchived))
        self.assertNotEqual(self.run_cleanup(seal=False).returncode, 0)
        self.assertTrue(self.old.exists() and unarchived.exists())

    def test_seal_rejects_transfer_with_incorrect_digest(self):
        self.seal()
        (self.work / 'plan.ready').rename(self.work / 'plan.ready.pending')
        result = subprocess.run(['sh', str(self.script), 'seal', str(self.work), '0' * 64],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.work / 'plan.ready').exists())

    def test_partial_inventory_cannot_begin_preparation(self):
        result = subprocess.run(['sh', str(self.script), 'begin', str(self.work)],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('inventory incomplete', result.stderr)

    def test_discard_verifies_flat_archive_and_handles_readonly_files_without_prompt(self):
        script = self.control_script

        class LocalAdb:
            def shell(self, *args):
                args = list(args)
                args[2] = script
                master, slave = os.openpty()
                try:
                    return subprocess.check_output(args, stdin=slave, stderr=slave,
                                                   text=True, timeout=3).rstrip('\n')
                finally:
                    os.close(master)
                    os.close(slave)

            def run(self, *args):
                shutil.copyfile(args[1], args[2])

        (self.work / 'boot-id').chmod(0o444)
        archive = self.root / 'archive'
        deploy.discard(LocalAdb(), str(self.work), archive)
        self.assertFalse(self.work.exists())
        self.assertEqual((archive / 'boot-id').read_text(), 'same-boot\n')
        self.assertTrue(self.current.exists() and self.recovery.exists() and self.old.exists())

    def test_discard_recovers_when_device_helper_is_missing_or_truncated(self):
        script = self.control_script

        class LocalAdb:
            def shell(self, *args):
                args = list(args)
                args[2] = script
                return subprocess.check_output(args, text=True).rstrip('\n')

            def run(self, *args):
                shutil.copyfile(args[1], args[2])

        for state in ('missing', 'truncated', 'empty', 'receipt-only'):
            with self.subTest(state=state):
                work = Path('/tmp/nx5-deploy-' + uuid.uuid4().hex)
                work.mkdir()
                try:
                    if state == 'truncated':
                        (work / 'device.sh').write_text('set -eu\nif')
                    if state in ('missing', 'truncated'):
                        (work / 'before.meminfo').write_text('partial transaction\n')
                    if state == 'receipt-only':
                        (work / 'purge.sha256').write_text('previous deletion receipt\n')
                    archive = self.root / work.name
                    deploy.discard(LocalAdb(), str(work), archive)
                    self.assertFalse(work.exists())
                    if state in ('missing', 'truncated'):
                        self.assertEqual((archive / 'before.meminfo').read_text(), 'partial transaction\n')
                    else:
                        self.assertEqual((archive / 'purge.sha256').read_text(), '\n')
                finally:
                    shutil.rmtree(work, ignore_errors=True)

    def test_control_changes_or_aliases_prevent_purge(self):
        def command(mode):
            return subprocess.run(['sh', str(self.script), mode, str(self.work)],
                                  capture_output=True, text=True)

        manifest = command('control-manifest')
        self.assertEqual(manifest.returncode, 0, manifest.stderr)
        (self.work / 'purge.sha256').write_text(manifest.stdout)
        (self.work / 'new-control').write_text('arrived after archive')
        self.assertNotEqual(command('purge').returncode, 0)
        (self.work / 'new-control').unlink()
        (self.work / 'zzz-alias').symlink_to('/dev/null')
        self.assertNotEqual(command('control-manifest').returncode, 0)
        self.assertNotEqual(command('purge').returncode, 0)
        self.assertTrue(self.work.exists())

    def test_changed_file_aborts_whole_plan_before_deletion(self):
        other = self.root / 'other'
        other.write_text('unchanged')
        (self.work / 'delete.sha256').write_text(self.lines(other, self.old))
        self.old.write_text('changed after archive')
        self.assertNotEqual(self.run_cleanup().returncode, 0)
        self.assertTrue(other.exists() and self.old.exists())

    def test_new_active_reference_is_preserved(self):
        process = self.proc / '123'
        (process / 'fd').mkdir(parents=True)
        (process / 'fd/1').symlink_to(self.old)
        result = self.run_cleanup()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('active or retained', result.stderr)
        self.assertTrue(self.old.exists())

    def test_lazy_relative_font_dependency_is_preserved(self):
        process = self.proc / '123'
        process.mkdir()
        (process / 'cwd').symlink_to(self.root)
        (process / 'environ').write_bytes(b'SLINT_DEFAULT_FONT=old file\0')
        result = self.run_cleanup()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('active or retained', result.stderr)
        self.assertTrue(self.old.exists())

    def test_device_alias_is_not_followed_or_deleted(self):
        self.old.unlink()
        self.old.symlink_to('/dev/null')
        self.assertNotEqual(self.run_cleanup().returncode, 0)
        self.assertTrue(self.old.is_symlink())

    def test_retained_directory_preserves_external_symlink_target(self):
        library = self.root / 'recovery-lib'
        library.mkdir()
        (library / 'libVideoDec.so').symlink_to(self.old)
        with (self.work / 'retain.paths').open('a') as target:
            target.write(str(library) + '\n')
        self.assertNotEqual(self.run_cleanup().returncode, 0)
        self.assertTrue(self.old.exists(), 'recovery library target was deleted')

    def test_retained_alias_retargeted_after_archive_is_protected(self):
        before = self.root / 'old-target'
        before.write_text('previous recovery dependency')
        alias = self.root / 'recovery-font'
        alias.symlink_to(before)
        with (self.work / 'retain.paths').open('a') as target:
            target.write(f'{alias}\n{before}\n')
        alias.unlink()
        alias.symlink_to(self.old)
        self.assertNotEqual(self.run_cleanup().returncode, 0)
        self.assertTrue(self.old.exists(), 'retargeted recovery dependency was deleted')

    def test_transitive_live_library_directory_links_and_cycles_preserve_target(self):
        first, second = self.root / 'first-lib', self.root / 'second-lib'
        first.mkdir()
        second.mkdir()
        (first / 'nested').symlink_to(second)
        (second / 'cycle').symlink_to(first)
        (second / 'libfixture.so').symlink_to(self.old)
        process = self.proc / '123'
        process.mkdir()
        (process / 'environ').write_bytes(f'LD_LIBRARY_PATH={first}\0'.encode())
        result = self.run_cleanup()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('active or retained', result.stderr)
        self.assertTrue(self.old.exists())

    def test_sigkill_does_not_leave_an_unowned_operation_lock(self):
        self.old.unlink()  # This transaction only exercises inventory.
        (self.work / 'boot-id').unlink()
        meminfo = self.proc / 'meminfo'
        meminfo.unlink()
        os.mkfifo(meminfo)
        child = subprocess.Popen(['sh', str(self.script), 'inventory', str(self.work)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 start_new_session=True)
        try:
            deadline = time.monotonic() + 3
            while not (self.work / 'boot-id').exists():
                self.assertIsNone(child.poll(), 'inventory exited before its blocking read')
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.01)
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=3)
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
        meminfo.unlink()
        meminfo.write_text('MemAvailable: 65536 kB\n')
        self.old.write_text('old file')
        # The next operation must be able to acquire the released lock.
        result = self.run_cleanup()
        self.assertNotIn('another inventory/cleanup', result.stderr)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_reboot_or_changed_recovery_aborts_cleanup(self):
        for failure in ('boot', 'recovery'):
            with self.subTest(failure=failure):
                target = self.work / 'boot-id' if failure == 'boot' else self.recovery
                before = target.read_bytes()
                target.write_text('changed')
                self.assertNotEqual(self.run_cleanup().returncode, 0)
                self.assertTrue(self.old.exists())
                target.write_bytes(before)


if __name__ == '__main__':
    unittest.main()
