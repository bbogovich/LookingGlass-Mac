"""Interactive, two-process co-op launcher for macOS (fork counterpart of run_coop_pair.ps1).

Usage: python3 tools/run_coop_pair.py --data <base-folder> [--menu | --local] [--map gvillage]

Each player gets separate settings and saves. Without --menu the host opens an
internet invite (or a local one with --local) and the guest joins it automatically.
Profiles and logs are kept in the printed private/coop/manual-<id> folder.
"""
import argparse
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
EXE_NAME = 'looking-glass.exe' if sys.platform == 'win32' else 'looking-glass'


def parse_args():
    """Read the options, rejecting the same combinations as the PowerShell launcher."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--data', required=True, help='original game base folder')
    parser.add_argument('--map', default='gvillage', help='first campaign map (default gvillage)')
    parser.add_argument('--exe', help='game executable (default target/release/looking-glass)')
    parser.add_argument('--local', action='store_true', help='connect on this Mac instead of over the internet')
    parser.add_argument('--menu', action='store_true', help='open two main menus and use Host and Join by hand')
    parser.add_argument('--combat', action='store_true', help='staged combat test (needs --map skool1)')
    args = parser.parse_args()
    if args.menu and args.local:
        parser.error('The in-game Host menu creates internet invites. Omit --local when using --menu.')
    if args.combat and (args.menu or args.map != 'skool1'):
        parser.error('The staged combat test uses skool1. Omit --menu and use --map skool1 with --combat.')
    return args


def wait_for_invite(invite_file, host):
    """Wait until the host has written its invite, or fail if it closes or takes too long."""
    deadline = time.monotonic() + 150
    while not invite_file.exists():
        if host.poll() is not None:
            sys.exit('The host closed before its invite was ready.')
        if time.monotonic() > deadline:
            sys.exit('The internet invite took too long to open.')
        time.sleep(0.1)


def stop_on_terminate(signum, frame):
    """Treat SIGTERM like Ctrl+C, so both windows are closed on the way out."""
    raise KeyboardInterrupt


def main():
    """Start the host and guest windows, then wait until both have closed."""
    args = parse_args()
    signal.signal(signal.SIGTERM, stop_on_terminate)
    executable = Path(args.exe).resolve() if args.exe else ROOT / 'target' / 'release' / EXE_NAME
    if not executable.is_file():
        sys.exit('Build Looking Glass first with cargo build --release --locked, or supply --exe.')
    data_path = Path(args.data).expanduser().resolve()
    session_folder = ROOT / 'private' / 'coop' / f'manual-{uuid.uuid4().hex}'
    session_folder.mkdir(parents=True)
    invite_file = session_folder / 'invite.txt'
    owned = []
    try:
        for role in ('host', 'guest'):
            if role == 'guest' and not args.menu:
                wait_for_invite(invite_file, owned[0])
            profile = session_folder / role
            profile.mkdir()
            # Arguments go straight to the process, never through a shell.
            arguments = ['--data', str(data_path), '--profile', str(profile), '--no-audio']
            if not args.menu:
                arguments += ['--map', args.map, f'--coop-{"host" if role == "host" else "join"}', str(invite_file)]
            if args.local and role == 'host':
                arguments.append('--coop-local')
            if args.combat and role == 'host':
                arguments.append('--combat-preview')
            with open(session_folder / f'{role}.log', 'wb') as out, \
                    open(session_folder / f'{role}-error.log', 'wb') as err:
                owned.append(subprocess.Popen([str(executable), *arguments], cwd=ROOT,
                                              stdin=subprocess.DEVNULL, stdout=out, stderr=err))
        if args.menu:
            print('Two independent main menus are open. Choose Co-op > Host in one, copy its invite, '
                  'then Co-op > Join in the other.')
        else:
            print('Two clients are open: Blue is the host; Red is the guest.')
        print('Click a window to control that Alice. ESC opens its menu; Q leaves co-op.')
        print('Each player has an independent profile. The host owns the shared campaign save.')
        print(f'Profiles and diagnostics: {session_folder}', flush=True)
        while any(process.poll() is None for process in owned):
            time.sleep(0.2)
    except KeyboardInterrupt:
        pass
    finally:
        for process in owned:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(5)
                except subprocess.TimeoutExpired:
                    process.kill()
        invite_file.unlink(missing_ok=True)


if __name__ == '__main__':
    main()
