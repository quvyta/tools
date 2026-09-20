# Changelog

Every release of quvyta-tools, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/); while the version starts with 0, a minor release may change the command line or the files qtools keeps under `~/.local/state/quvyta-tools/`, and the notes say so when it does.

## 0.1.6 - 2026-09-20

### Added

- The Hardware group, with four items. Compressed swap in RAM (`zram-swap`): installs `zram-generator` and writes a configuration file of its own, adding a compressed swap device half the size of the memory on top of any swap already there. Bluetooth (`bluetooth`), only on a machine that has a controller: `bluez`, its tools and the service. Laptop power management (`laptop-power`), only with a battery and only when `power-profiles-daemon` is not already managing power: `tlp` at its own defaults. NVIDIA and Wayland (`nvidia-wayland`), only with an NVIDIA card whose driver is already installed: kernel modesetting in a modprobe file of qtools' own, plus the suspend, resume and hibernate services.

### Changed

- Built on quvyta-framework 0.1.10.
- The published package lists its files from the root of the repository, so a file of the same name in a subfolder can no longer slip into it.

### Notes

- qtools does not install a graphics driver and does not touch the bootloader or the initramfs. Which NVIDIA driver is right depends on the card and the kernel, and a wrong one can leave a machine without a display; a bootloader that will not boot cannot be undone from inside qtools.

## 0.1.5 - 2026-09-19

### Added

- A moving picture at the top of the README (also as MP4): checking two items, the confirmation, the run and the Security group. It is drawn from the test harness with a stand-in for the run, so nothing on it comes from a real machine.

### Changed

- When a run ends, its outcome (Done or It failed) stays beside the run's title instead of showing as a notification that fades; after a failure, the line under the title says the output explains why and that what was done can still be undone.

### Fixed

- The notification at the end of a run no longer covers the terminal's note in its bottom corner that gives the exit code.

## 0.1.4 - 2026-09-19

### Added

- Two new interface languages: Simplified Chinese and Japanese. Wide characters stay whole everywhere on the screen, and long Chinese and Japanese lines break between characters instead of running off the edge.
- README: a one-line summary and badges at the top, and the exit code for an unsupported distribution.
- This changelog, a contributing guide and issue templates.
- The package lists its homepage, <https://quvyta.com/qtools/>.

### Changed

- The system language is now read with its region, so `pt_BR.UTF-8` gives Brazilian Portuguese and `zh_CN.UTF-8` Simplified Chinese.
- Plural forms follow the full language code of the chosen language.
- Built on quvyta-framework 0.1.8.

### Fixed

- The key names in the bar at the bottom of the screen (such as `quit`) now appear in every interface language, not only in English and Turkish.

## 0.1.3 - 2026-09-18

### Added

- Five new interface languages: German, Spanish, French, Brazilian Portuguese and Russian, next to English and Turkish. Every language file is checked against the English one (the same keys, the same placeholders, the plural forms the language uses), and every language is drawn at three screen sizes to make sure nothing overflows or loses its alignment.
- Screenshots at the top of the README: the list, the Security group's detail, the confirmation and a narrow terminal.

### Changed

- Built on quvyta-framework 0.1.5.

### Known issues

- Brazilian Portuguese is not yet picked up from a regional setting such as `pt_BR.UTF-8`; qtools shows English for it.
- In the five new languages a few key names in the bar at the bottom of the screen (such as `quit`) are still in English.

## 0.1.2 - 2026-09-18

### Changed

- README: a one-line install command, installing with cargo, and what to do when `qtools` is not found on the PATH.
- Built on quvyta-framework 0.1.4.

## 0.1.1 - 2026-09-18

### Added

- qtools checks the distribution when it starts. It runs on Arch Linux and on distributions built on it (`ID=arch`, or `arch` in `ID_LIKE` in `/etc/os-release`). Anywhere else the screen shows only a notice that the distribution is not supported yet, and the command line exits with code 3; nothing is changed.

## 0.1.0 - 2026-09-18

The first beta.

### Added

- One list of recommended Arch Linux settings, in groups:
  - Packages: mirror list (`reflector` and its weekly timer), pacman options (`ParallelDownloads`, `Color`, `VerbosePkgLists`), the multilib repository, an AUR helper (`paru`) and cache cleanup (`paccache.timer`).
  - Security: a firewall (`ufw`, incoming blocked, outgoing allowed) and SSH hardening (no password or root logins, only when `openssh` is installed).
  - Maintenance: SSD trim (`fstrim.timer`), a 500 MB journal size limit and time sync (`systemd-timesyncd`).
- Each item shows where it stands on this machine (`● Applied`, `○ Off`, `◐ Half`, `▲ Changed`) and, beside the list, every package, file and service it touches. An item that makes no sense on this machine is shown faint, with the reason.
- One confirmation before anything runs: the commands, the files, where the backup goes and how many steps ask for a password.
- Several items can be checked and applied one after the other; any applied item can be undone.
- A backup of every file before qtools writes it, and a journal (`applied.toml`) with a digest of what was written, so a file changed behind qtools' back is shown as `▲ Changed` and never silently overwritten.
- Undo removes only the packages qtools itself installed.
- Steps that need privileges run through `sudo` in a terminal embedded in the screen; qtools never runs as root and never sees your password.
- Running an item again changes nothing and says it is already applied.
- `qtools --run <id>...` and `qtools --revert <id>...` apply or undo items without the screen.
- Below 72 columns the screen folds: the groups become a strip above the list and the detail opens below it.
- Mouse support throughout.
- English and Turkish.
- Two commands that do the same thing: `qtools` and `quvyta-tools`.
