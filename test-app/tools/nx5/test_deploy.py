import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
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
        raise AssertionError(args)

    def run(self, *args):
        self.calls.append(args)
        if args[0] == 'pull':
            Path(args[2]).write_bytes(b'corrupt' if self.corrupt else self.files[args[1]])
        elif args[0] != 'push':
            raise AssertionError(args)
        return ''


class HostTests(unittest.TestCase):
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
            self.assertEqual(sum(c[0] == 'push' for c in device.calls), 3)

    def test_corrupt_archive_never_issues_cleanup_files(self):
        device = ArchiveDevice()
        device.corrupt = True
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy = root / 'retain.json'
            policy.write_text(json.dumps(device.policy))
            with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                deploy.prepare(device, device.work, policy, root / 'archive')
            self.assertFalse(any(c[0] == 'push' for c in device.calls))
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
        with tempfile.TemporaryDirectory() as directory, patch.object(adb, 'shell') as shell, \
                patch.object(adb, 'run', side_effect=OSError('transfer failed')):
            with self.assertRaisesRegex(OSError, 'transfer failed'):
                deploy.discard(adb, ArchiveDevice.work, Path(directory) / 'archive')
            self.assertFalse(any(call.args[0] == 'rm' for call in shell.call_args_list))

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

    def test_finish_removes_readonly_control_files_without_terminal_prompt(self):
        # Older adbd gives commands a terminal. rm without -f prompts for the
        # root-created read-only files even when the ADB user owns their directory.
        class LocalAdb:
            def read(self, path):
                return Path(path).read_text().rstrip('\n')

            def shell(self, *args):
                master, slave = os.openpty()
                try:
                    with subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave) as child:
                        try:
                            self_status = child.wait(timeout=1)
                        except subprocess.TimeoutExpired:
                            child.kill()
                            raise
                    if self_status:
                        raise RuntimeError('control cleanup failed')
                finally:
                    os.close(master)
                    os.close(slave)

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            work = root / 'control'
            work.mkdir()
            for name in ('before.meminfo', 'after.meminfo', 'protected.paths',
                         'protected-now.paths', 'cleaned'):
                file = work / name
                file.write_text('archived\n')
                file.chmod(0o444)
            with patch.object(deploy, 'load_plan', return_value={'work': str(work)}):
                deploy.finish(LocalAdb(), root / 'plan.json')
            self.assertFalse(work.exists())
            self.assertEqual((root / 'cleaned').read_text(), 'archived\n')


class DeviceCleanupTests(unittest.TestCase):
    """Execute the actual cleanup shell, with only root/proc paths redirected."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='slint-bridge-deploy-test-')
        self.root = Path(self.directory.name)
        self.work = Path(tempfile.mkdtemp(prefix='nx5-deploy-'))
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

    def tearDown(self):
        import shutil
        shutil.rmtree(self.work)
        self.directory.cleanup()

    def lines(self, *paths):
        return ''.join(f'{deploy.digest(p)}  {p}\n' for p in paths)

    def run_cleanup(self):
        return subprocess.run(['sh', str(self.script), 'cleanup', str(self.work)],
                              capture_output=True, text=True)

    def test_deletes_only_verified_file_and_retains_binaries(self):
        result = self.run_cleanup()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.old.exists())
        self.assertTrue(self.current.exists() and self.recovery.exists())
        self.assertEqual((self.work / 'cleaned').read_text(), 'same-boot\n')

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
