#!/bin/bash
# The microcode item on a throwaway Arch container: the real release binary, the real pacman
# and the real mkinitcpio, and nothing of the machine it runs on. Run it from the repository
# root:
#
#     bash tests/container/microcode.sh
#
# It builds the release binary, starts `docker.io/library/archlinux:latest`, mounts that
# binary read-only and one temporary folder writable for the transcript, and takes the
# container away with it. `cargo test` does not run this: it needs a container runtime and
# the network. Nothing outside the worktree and that temporary folder is written.
set -eu
export LC_ALL=C

root=$(pwd)
[ -f Cargo.toml ] || { echo "run this from the repository root" >&2; exit 1; }

# Where podman keeps its images and its runtime state. The default is under the worktree's
# own `target`, so a machine whose temporary folder is small still has room for them; point
# them anywhere else with these two variables.
PODMAN_ROOT=${PODMAN_ROOT:-$root/target/podman/root}
PODMAN_RUNROOT=${PODMAN_RUNROOT:-$root/target/podman/runroot}
IMAGE=${IMAGE:-docker.io/library/archlinux:latest}

podman() {
    command podman --root "$PODMAN_ROOT" --runroot "$PODMAN_RUNROOT" "$@"
}

# A folder of this run's own, so two runs never share or remove each other's transcript.
# The container's root writes into it, so it is opened to every user inside the container;
# nothing outside this folder is opened.
work=$(mktemp -d "${TMPDIR:-/tmp}/qtools-microcode.XXXXXX")
chmod 777 "$work"

echo "== building the release binary"
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo build --release --bin qtools --message-format=json \
    >"$work/build.json"
# Cargo says where the binary is, so the script works wherever the target directory is.
binary=$(sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' "$work/build.json" | tail -1)
[ -x "$binary" ] || { echo "cargo built no qtools binary" >&2; tail -5 "$work/build.json" >&2; exit 1; }
echo "   $binary"

if podman image exists "$IMAGE"; then
    echo "== $IMAGE is already here"
else
    echo "== pulling $IMAGE"
    podman pull "$IMAGE" >/dev/null
fi

# The container's part, read from standard input so the transcript is the only thing it
# writes outside itself. `script` gives the run a terminal of its own, the way the screen's
# embedded terminal does, and leaves the whole run's output in the transcript.
podman run --rm -i -v "$binary:/usr/local/bin/qtools:ro" -v "$work:/transcript" "$IMAGE" sh -s <<'CONTAINER'
set -eu
export LC_ALL=C
export DEBIAN_FRONTEND=noninteractive

echo
echo "== inside the container: $(uname -m), $(. /etc/os-release && echo "$PRETTY_NAME")"
pacman -Syu --noconfirm --needed mkinitcpio linux sudo >/dev/null
echo "mkinitcpio $(pacman -Q mkinitcpio | cut -d' ' -f2), kernel $(pacman -Q linux | cut -d' ' -f2)"
echo "processor: $(grep -m1 vendor_id /proc/cpuinfo)"

# The starting point: Arch's own configuration, with the microcode hook taken out by hand.
sed -i 's/autodetect microcode modconf/autodetect modconf/' /etc/mkinitcpio.conf
cp /etc/mkinitcpio.conf /transcript/mkinitcpio.conf.before
echo "before: $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf)"

# A normal user with a sudoers rule of its own, and nothing of qtools' in root's home.
useradd -m -s /bin/bash tester
printf 'tester ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/tester
chmod 440 /etc/sudoers.d/tester

echo
echo "== first run, as tester: qtools --run microcode"
su tester -c "script -q -e -c 'qtools --run microcode' /transcript/run1.log < /dev/null" || true
echo "-- what qtools printed"
grep -v '^Script ' /transcript/run1.log | sed 's/^/   /'
echo "after:  $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf)"
echo "-- what the item installed and what it kept"
echo "   $(pacman -Q intel-ucode 2>/dev/null || pacman -Q amd-ucode 2>/dev/null || echo 'no microcode package')"
ls -l /home/tester/.local/state/quvyta-tools /home/tester/.local/state/quvyta-tools/backups | sed 's/^/   /'
echo "-- the initramfs"
ls -l /boot/initramfs-linux.img
echo "-- the microcode in it, as lsinitcpio reads the image"
lsinitcpio /boot/initramfs-linux.img | head -12 | sed 's/^/   /'
lsinitcpio /boot/initramfs-linux.img | grep -c 'kernel/x86/microcode' | sed 's/^/   microcode files: /'
stat -c '   %n built at %y' /boot/initramfs-linux.img
cp /etc/mkinitcpio.conf /transcript/mkinitcpio.conf.after
first=$(stat -c %Y /boot/initramfs-linux.img)

echo
echo "== second run, as tester: nothing is left to do"
su tester -c "script -q -e -c 'qtools --run microcode' /transcript/run2.log < /dev/null" || true
grep -v '^Script ' /transcript/run2.log | sed 's/^/   /'
second=$(stat -c %Y /boot/initramfs-linux.img)
echo "   the image was built at $first and is now at $second"
[ "$first" = "$second" ] || { echo "the second run built the image again" >&2; exit 1; }
if grep -q 'Image generation successful' /transcript/run2.log; then
    echo "the second run built an image" >&2
    exit 1
fi
echo "   no image was built a second time"
cmp /etc/mkinitcpio.conf /transcript/mkinitcpio.conf.after && echo "   the configuration did not move either"
echo "-- what qtools has on record, and the same three steps as after the first run"
sed 's/^/   /' /home/tester/.local/state/quvyta-tools/applied.toml

echo
echo "== undo, as tester: qtools --revert microcode"
su tester -c "script -q -e -c 'qtools --revert microcode' /transcript/run3.log < /dev/null" || true
grep -v '^Script ' /transcript/run3.log | sed 's/^/   /'
echo "after the undo: $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf)"
cmp /etc/mkinitcpio.conf /transcript/mkinitcpio.conf.before && echo "   /etc/mkinitcpio.conf is byte for byte what it was"
echo "-- the package the item installed is gone again"
pacman -Q intel-ucode 2>/dev/null || pacman -Q amd-ucode 2>/dev/null || echo "   no microcode package is installed"
echo "-- the image was built again, this time from the configuration that is back"
ls -l /boot/initramfs-linux.img
if lsinitcpio /boot/initramfs-linux.img | grep -q 'kernel/x86/microcode'; then
    echo "   the image still carries the microcode" >&2
    exit 1
fi
echo "   no microcode file is left in the image"
echo "-- the boot setup was never written to"
ls -l /boot
echo "-- qtools' own record of what it did"
sed 's/^/   /' /home/tester/.local/state/quvyta-tools/applied.toml 2>/dev/null || echo "   (the journal is empty: everything was undone)"

echo
echo "== a machine that keeps HOOKS in a drop-in: the drop-in is what is written"
mkdir -p /etc/mkinitcpio.conf.d
sed 's/ microcode / /' /etc/mkinitcpio.conf > /etc/mkinitcpio.conf.d/50-site.conf
cp /etc/mkinitcpio.conf.d/50-site.conf /transcript/dropin.before
echo "   main:    $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf)"
echo "   drop-in: $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf.d/50-site.conf)"
su tester -c "script -q -e -c 'qtools --run microcode' /transcript/run4.log < /dev/null" || true
grep -v '^Script ' /transcript/run4.log | sed 's/^/   /'
echo "   main:    $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf)"
echo "   drop-in: $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf.d/50-site.conf)"
cmp /etc/mkinitcpio.conf /transcript/mkinitcpio.conf.before \
    && echo "   the main file is byte for byte what it was: the drop-in is the one that sets the list"
su tester -c "script -q -e -c 'qtools --revert microcode' /transcript/run5.log < /dev/null" || true
grep -v '^Script ' /transcript/run5.log | sed 's/^/   /'
echo "   drop-in after the undo: $(grep -m1 '^HOOKS=' /etc/mkinitcpio.conf.d/50-site.conf)"
cmp /etc/mkinitcpio.conf.d/50-site.conf /transcript/dropin.before \
    && echo "   the drop-in is byte for byte what it was"
echo
echo "== the container is finished"
CONTAINER

echo
echo "== transcript kept in $work"
ls -l "$work"
