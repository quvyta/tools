#!/bin/bash
# The catalog on a throwaway Arch container: the real release binary, the real pacman,
# reflector and pacman.conf, and nothing of the machine it runs on. Run it from the
# repository root:
#
#     bash tests/container/catalog.sh
#
# What it shows, each with the real files:
#   - an item whose file is the first of its folder (the journal limit) is written, and the
#     folder is made for it;
#   - pacman's options land in [options] of Arch's own pacman.conf, which pacman then reads
#     without a word, and come back out byte for byte;
#   - two items that write the same file are undone newest first, and the older one refuses
#     while the newer one's change is still on it;
#   - an undo whose backup is gone touches nothing and says so;
#   - the mirrors' country goes into the file reflector.timer reads;
#   - an undo of an item qtools did not do runs nothing, in the language of the terminal.
#   - the SSH rule in the firewall is added once, undone, and a rule the person made is left
#     alone by both the run and the undo;
#   - an item that does not fit the machine is refused on the command line, nothing installed.
#
# The container has no systemd running, so a step that starts a service fails there; the
# transcript says where, and the undo then takes back what was done before it.
#
# `cargo test` does not run this: it needs a container runtime and the network. Nothing
# outside the worktree and a temporary folder of the run's own is written.
set -eu
export LC_ALL=C

root=$(pwd)
[ -f Cargo.toml ] || { echo "run this from the repository root" >&2; exit 1; }

PODMAN_ROOT=${PODMAN_ROOT:-$root/target/podman/root}
PODMAN_RUNROOT=${PODMAN_RUNROOT:-$root/target/podman/runroot}
IMAGE=${IMAGE:-docker.io/library/archlinux:latest}

podman() {
    command podman --root "$PODMAN_ROOT" --runroot "$PODMAN_RUNROOT" "$@"
}

# A folder of this run's own, opened to the container's users, who write the transcript.
work=$(mktemp -d "${TMPDIR:-/tmp}/qtools-catalog.XXXXXX")
chmod 777 "$work"

echo "== building the release binary"
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo build --release --bin qtools --message-format=json \
    >"$work/build.json"
binary=$(sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' "$work/build.json" | tail -1)
[ -x "$binary" ] || { echo "cargo built no qtools binary" >&2; tail -5 "$work/build.json" >&2; exit 1; }
echo "   $binary"

if podman image exists "$IMAGE"; then
    echo "== $IMAGE is already here"
else
    echo "== pulling $IMAGE"
    podman pull "$IMAGE" >/dev/null
fi

podman run --rm -i -v "$binary:/usr/local/bin/qtools:ro" -v "$work:/transcript" "$IMAGE" bash -s <<'CONTAINER'
set -eu
export LC_ALL=C

fail() { echo "FAILED: $*" >&2; exit 1; }
# Runs qtools as the tester in a terminal of its own, the way the screen's embedded terminal
# does, and shows what it printed.
as_tester() { # name, then the arguments
    local name=$1; shift
    su tester -c "script -q -e -c 'qtools $*' /transcript/$name.log < /dev/null > /dev/null" || true
    grep -v '^Script ' "/transcript/$name.log" | sed 's/^/   /'
}

echo
echo "== inside the container: $(. /etc/os-release && echo "$PRETTY_NAME")"
pacman -Syu --noconfirm --needed sudo util-linux diffutils >/dev/null
useradd -m -s /bin/bash tester
printf 'tester ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/tester
chmod 440 /etc/sudoers.d/tester
cp /etc/pacman.conf /transcript/pacman.conf.before
state=/home/tester/.local/state/quvyta-tools

echo
echo "== 1. the journal limit, whose folder a clean Arch does not have"
[ -e /etc/systemd/journald.conf.d ] && echo "   (the folder is already there)" || echo "   /etc/systemd/journald.conf.d is not there"
as_tester limit --run journal-limit
[ -f /etc/systemd/journald.conf.d/50-quvyta-tools.conf ] || fail "the drop-in was not written"
sed 's/^/   drop-in: /' /etc/systemd/journald.conf.d/50-quvyta-tools.conf
as_tester limit-undo --revert journal-limit
[ -e /etc/systemd/journald.conf.d/50-quvyta-tools.conf ] && fail "the undo left the drop-in"
echo "   the undo took the drop-in away"

echo
echo "== 2. pacman options in Arch's own pacman.conf"
as_tester options --run pacman-options
for option in ParallelDownloads Color VerbosePkgLists; do
    echo "   pacman-conf $option: $(pacman-conf "$option" 2>&1 || echo '(not set)')"
done
pacman-conf --repo-list >/dev/null 2>/transcript/pacman-conf.err || true
[ -s /transcript/pacman-conf.err ] && { cat /transcript/pacman-conf.err; fail "pacman complains about the file"; }
echo "   pacman reads the file without a word"
tail -n 12 /etc/pacman.conf | grep -E 'ParallelDownloads|^Color|VerbosePkgLists' && fail "an option landed under a repository"
echo "   no option under a repository"

echo
echo "== 3. a second item on the same file, and the undo order"
as_tester multilib --run multilib
pacman-conf --repo-list | grep -qx multilib || fail "multilib is not a repository"
echo "   multilib is a repository now"
echo "-- the older item first: its file has the newer item's change on it"
as_tester refused --revert pacman-options
cmp -s /etc/pacman.conf /transcript/pacman.conf.before && fail "the refused undo changed the file"
echo "-- newest first, then the older one"
as_tester multilib-undo --revert multilib
as_tester options-undo --revert pacman-options
cmp /etc/pacman.conf /transcript/pacman.conf.before || fail "pacman.conf is not what it was"
echo "   /etc/pacman.conf is byte for byte what it was"

echo
echo "== 4. an undo whose backup is gone"
as_tester options-again --run pacman-options
cp /etc/pacman.conf /transcript/pacman.conf.applied
rm -f "$state"/backups/pacman-options-*
as_tester lost --revert pacman-options
cmp /etc/pacman.conf /transcript/pacman.conf.applied || fail "the undo touched the file without its backup"
[ -s /etc/pacman.conf ] || fail "pacman.conf was written empty"
echo "   the file is as qtools left it, not empty"
cp /transcript/pacman.conf.before /etc/pacman.conf
rm -f "$state/applied.toml"

echo
echo "== 5. the mirrors, with the country in the file reflector.timer reads"
su tester -c "unset LC_ALL; LANG=de_DE.UTF-8 script -q -e -c 'qtools --run mirrors' /transcript/mirrors.log < /dev/null > /dev/null" || true
grep -v '^Script ' /transcript/mirrors.log | sed 's/^/   /'
sed 's/^/   reflector.conf: /' /etc/xdg/reflector/reflector.conf
grep -qx -- '--country DE' /etc/xdg/reflector/reflector.conf || fail "the country is not in reflector.conf"
head -n 3 /etc/pacman.d/mirrorlist | sed 's/^/   mirrorlist: /'
as_tester mirrors-undo --revert mirrors
sed 's/^/   reflector.conf after the undo: /' /etc/xdg/reflector/reflector.conf 2>/dev/null || echo "   reflector.conf is gone with its package"

echo
echo "== 6. an undo of what qtools did not do, in Turkish"
su tester -c "unset LC_ALL; LANG=tr_TR.UTF-8 script -q -e -c 'qtools --revert time-sync' /transcript/nothing.log < /dev/null > /dev/null" || true
grep -v '^Script ' /transcript/nothing.log | sed 's/^/   /'
grep -q 'qtools yapmadı' /transcript/nothing.log || fail "the refusal is not in Turkish"

echo
echo "== 7. SSH through the firewall: the rule qtools adds, and one the person made"
pacman -S --noconfirm --needed openssh ufw >/dev/null
ls -l /etc/ufw/user.rules | sed 's/^/   /'
su tester -c 'test -r /etc/ufw/user.rules' || fail "the tester cannot read ufw's rules, which the check needs"
echo "   ufw's rules are readable without root"
as_tester ssh-rule --run firewall-ssh
ufw show added | sed 's/^/   ufw show added: /'
ufw show added | grep -qx 'ufw allow 22/tcp' || fail "the rule is not there"
su tester -c 'test -r /etc/ufw/user.rules' || fail "writing the rule made ufw's rules unreadable"
as_tester ssh-rule-again --run firewall-ssh
[ "$(ufw show added | grep -cx 'ufw allow 22/tcp')" = 1 ] || fail "a second run added the rule again"
as_tester ssh-rule-undo --revert firewall-ssh
ufw show added | grep -qx 'ufw allow 22/tcp' && fail "the undo left the rule"
echo "   the undo took the rule away"
echo "-- a rule the person made is left alone, and undo has nothing of qtools' to take"
ufw allow 22/tcp >/dev/null
as_tester ssh-rule-own --run firewall-ssh
as_tester ssh-rule-own-undo --revert firewall-ssh
ufw show added | grep -qx 'ufw allow 22/tcp' || fail "the person's own rule was removed"
echo "   the person's rule is still there"

echo
echo "== 8. an item that does not fit this machine, asked for on the command line"
as_tester bluetooth --run bluetooth
pacman -Q bluez >/dev/null 2>&1 && fail "bluez was installed on a machine without a controller"
echo "   nothing was installed"

echo
echo "== the container is finished"
CONTAINER

echo
echo "== transcript kept in $work"
