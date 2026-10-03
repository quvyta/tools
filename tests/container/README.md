# The container checks

Scripts that run qtools for real — the release binary, pacman, mkinitcpio — inside a
throwaway Arch container, so nothing of the machine they are run on is touched. `cargo test`
does not run them: they need a container runtime and the network.

| Script | What it shows |
|---|---|
| `microcode.sh` | The `microcode` item on a fresh Arch with `mkinitcpio` and a kernel: the package, the `HOOKS` line, the rebuilt initramfs with the microcode in its early part, a second run that does nothing, an undo that puts `/etc/mkinitcpio.conf` back byte for byte, and the same again on a machine that keeps `HOOKS` in a drop-in, where the drop-in is what gets written. |
| `catalog.sh` | The catalog's own files on a fresh Arch: the journal limit written into a folder a clean Arch does not have, pacman's options in `[options]` of Arch's own `pacman.conf` (pacman reads it without a word), two items on the same file undone newest first while the older one refuses first, an undo whose backup is gone leaving the file as it is, the mirrors' country in the file `reflector.timer` reads, an undo of what qtools did not do refused in Turkish, the SSH rule in the firewall added once and undone while a rule the person made stays, and `--run bluetooth` refused on a machine without a controller. |

Run one from the repository root:

```
bash tests/container/microcode.sh
bash tests/container/catalog.sh
```

The image is pulled if it is not there (`docker.io/library/archlinux:latest`). podman's
images and runtime state are kept under this worktree's own `target/podman`, so a machine
with a small temporary folder still has room; set `PODMAN_ROOT` and `PODMAN_RUNROOT` to
move them anywhere else. Nothing outside the worktree and the script's own temporary folder
is written, and the container is taken away when it is finished. The transcript is left in
that temporary folder, a new one for every run, whose name the script prints at the end.
