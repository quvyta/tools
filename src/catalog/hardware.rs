//! Microcode, graphics, power and swap.

use std::path::Path;

use crate::step::Step;
use crate::system::{Cmd, System};
use crate::tweak::{Availability, Group, Tweak};

/// Always fits: every machine can use more usable memory.
fn always(_: &mut dyn System) -> Availability {
    Availability::Yes
}

/// The processor's vendor, as a machine names it in `/proc/cpuinfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cpu {
    /// Intel, whose microcode package is `intel-ucode`.
    Intel,
    /// AMD, whose microcode package is `amd-ucode`.
    Amd,
    /// Any other vendor, and any machine that will not say. The default, because it is the
    /// answer whenever nothing has been read: qtools has no microcode package for it and
    /// would rather say so than guess.
    #[default]
    Other,
}

/// The machine's processor, read the way mkinitcpio's own `microcode` hook reads it: the
/// first `vendor_id` line of `/proc/cpuinfo`. An x86 processor has one; a board that is not
/// x86, and a sandbox that cannot see the file at all, answer [`Cpu::Other`].
pub fn cpu(system: &mut dyn System) -> Cpu {
    let Ok(Some(text)) = system.read(Path::new(CPUINFO)) else { return Cpu::Other };
    let vendor = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == "vendor_id").then(|| value.trim())
    });
    match vendor {
        Some("GenuineIntel") => Cpu::Intel,
        Some("AuthenticAMD") => Cpu::Amd,
        _ => Cpu::Other,
    }
}

const CPUINFO: &str = "/proc/cpuinfo";

/// The processor's microcode, put in the initramfs by mkinitcpio's own `microcode` hook.
///
/// The hook has been in the default `HOOKS` of `/etc/mkinitcpio.conf` since mkinitcpio 36,
/// which is what lets the item do its work without touching the bootloader: the microcode
/// ends up inside the initramfs the bootloader already boots, so no `initrd` line has to be
/// added to a systemd-boot entry or to GRUB. A bootloader is not written, not read for
/// writing, and not even named in the preview — a wrong change there is a machine that does
/// not start, and no backup on earth brings such a machine back.
///
/// The item is built for one vendor and both halves of it come from the same reading: the
/// steps install that vendor's package, and the rule that decides whether the machine fits
/// asks the machine whether it is that vendor. A catalog built for one machine and run on
/// another therefore finds the item unavailable rather than installing the wrong package,
/// which is why `cpu` is a parameter here and not a lookup inside the steps: the steps name
/// their packages literally, and nothing about them is read at the moment they run.
///
/// The three steps, in order: the package, the hook, and the initramfs. The hook goes right
/// after `autodetect` — Arch's own order is `base udev autodetect microcode modconf ...` —
/// and after `udev` or `systemd` when there is no `autodetect` to follow, because the hook
/// with `autodetect` in front of it adds only the microcode files of the processor the image
/// was built on and without it adds the whole archive of both vendors. The file it is put
/// in is the one mkinitcpio really reads the list from, which is a drop-in rather than the
/// main file on a machine that keeps `HOOKS` in one, and only the item's own place in the
/// list moves: the rest of the line, and the rest of the file, are left byte for byte.
pub fn microcode(cpu: Cpu) -> Tweak {
    let (package, applies_to): (&'static str, fn(&mut dyn System) -> Availability) = match cpu {
        Cpu::Intel => (INTEL_UCODE, fits_intel),
        Cpu::Amd => (AMD_UCODE, fits_amd),
        Cpu::Other => return no_microcode_package(),
    };
    Tweak {
        id: "microcode",
        group: Group::Hardware,
        options: Vec::new(),
        steps: vec![
            Step::PackageInstalled { package },
            Step::ListItemAdded {
                file: MKINITCPIO_CONF,
                drop_ins: MKINITCPIO_CONF_D,
                key: "HOOKS",
                item: "microcode",
                after: AFTER_MICROCODE,
            },
            rebuild_initramfs_after(None),
        ],
        applies_to,
    }
}

const INTEL_UCODE: &str = "intel-ucode";
const AMD_UCODE: &str = "amd-ucode";
const MKINITCPIO_CONF: &str = "/etc/mkinitcpio.conf";
const MKINITCPIO_CONF_D: &str = "/etc/mkinitcpio.conf.d";

/// The `microcode` install hook, which arrived in mkinitcpio 36. Read as text: it is a
/// shell script qtools never runs, and its being there is the whole answer.
const MICROCODE_HOOK: &str = "/usr/lib/initcpio/install/microcode";

/// Where the item sits in the list: after `autodetect` when the machine has it, and after
/// `udev` (or the `systemd` hook that replaced it) when it does not. `autodetect` is first
/// because it is the case Arch's own configuration is in, and it is the one that matters:
/// with `autodetect` in front of it the hook adds only the microcode of the processor the
/// image is built on, and without it the hook adds the whole archive of both vendors.
const AFTER_MICROCODE: &[&str] = &["autodetect", "udev", "systemd"];

/// Rebuilding every initramfs the machine's presets name, so the image matches the hook
/// that was just put in place.
///
/// Whether that is already the case is asked of the files' own timestamps rather than of the
/// images' contents: an initramfs that is newer than the newest configuration file was built
/// from a configuration at least as new as that one, which is the same question Arch's own
/// initramfs hooks ask before they rebuild, and it costs one directory listing instead of a
/// rebuild. Right after the package or the hook step has changed something, the answer is
/// certainly no, so the image is built once and not again; a second run finds the image newer
/// than the configuration and does nothing at all, which is also what happens on a machine
/// whose kernel was upgraded in between, since that upgrade built the image again. A hand
/// edit of the configuration turns the answer to no once more, and rebuilding is what
/// mkinitcpio itself would do then. A machine with no initramfs image to compare with is
/// answered no as well, and `mkinitcpio -P` decides for itself what there is to build.
fn rebuild_initramfs_after(also: Option<&str>) -> Step {
    Step::CommandRun {
        run: Cmd::new("mkinitcpio", ["-P"]).root(),
        check: Cmd::new("sh", ["-c", &initramfs_is_current(also)]),
        undo: Cmd::new("mkinitcpio", ["-P"]).root(),
        label: "mkinitcpio -P",
    }
}

/// The question the rebuild asks, in one line: is an initramfs image at least as new as the
/// configuration it would be built from? The newest of the main file, its drop-ins and `also`
/// — a file of `/etc/modprobe.d` the image copies — is the one that counts.
fn initramfs_is_current(also: Option<&str>) -> String {
    let files = match also {
        Some(also) => format!("/etc/mkinitcpio.conf /etc/mkinitcpio.conf.d/*.conf {also}"),
        None => "/etc/mkinitcpio.conf /etc/mkinitcpio.conf.d/*.conf".to_owned(),
    };
    format!(
        "conf=$(ls -t {files} 2>/dev/null | head -n 1); [ -n \"$conf\" ] && [ -n \"$(find /boot -maxdepth 1 -name 'initramfs-*.img' -newer \"$conf\" -print -quit)\" ]"
    )
}

/// The item for a processor qtools has no microcode package for. It is in the list to say
/// so, and it has nothing to do: there is no package to install and no hook worth adding
/// for a machine whose vendor is neither Intel's nor AMD's.
fn no_microcode_package() -> Tweak {
    Tweak { id: "microcode", group: Group::Hardware, options: Vec::new(), steps: Vec::new(), applies_to: fits_no_cpu }
}

/// Whether the machine is an Intel one that mkinitcpio can put the microcode in for.
fn fits_intel(system: &mut dyn System) -> Availability {
    fits_microcode(Cpu::Intel, system)
}

/// Whether the machine is an AMD one that mkinitcpio can put the microcode in for.
fn fits_amd(system: &mut dyn System) -> Availability {
    fits_microcode(Cpu::Amd, system)
}

/// Everything the item needs from a machine, each with its own reason: a processor with a
/// package qtools knows, and an mkinitcpio that can put it in the initramfs on its own.
/// What a person does about each of them is a different thing, so each is said apart.
fn fits_microcode(wanted: Cpu, system: &mut dyn System) -> Availability {
    if cpu(system) != wanted {
        return Availability::No("unavailable.no-cpu-microcode");
    }
    if system.read(Path::new(MKINITCPIO_CONF)).ok().flatten().is_none() {
        // dracut, booster, a machine that boots nothing but a kernel image: the initramfs
        // on this machine is not mkinitcpio's to write.
        return Availability::No("unavailable.no-mkinitcpio");
    }
    if system.read(Path::new(MICROCODE_HOOK)).ok().flatten().is_none() {
        // Before mkinitcpio 36 the hook did not exist and the microcode had to be named as
        // a second `initrd` in the bootloader itself, which is a bootloader qtools does not
        // edit. The Arch wiki page "Microcode" is where that is written down.
        return Availability::No("unavailable.mkinitcpio-too-old");
    }
    Availability::Yes
}

/// Never fits: the item was built for a processor qtools has no microcode package for.
fn fits_no_cpu(_: &mut dyn System) -> Availability {
    Availability::No("unavailable.no-cpu-microcode")
}

/// Compressed swap in RAM, through the generator systemd ships as a separate package.
///
/// The formula in the drop-in (`min(ram / 2, 8192)`) is zram-generator's own syntax; it is
/// written once here rather than computed from the machine's memory, so the same file is
/// produced on every machine and a second run always finds exactly what it wrote. This adds
/// a swap device rather than replacing one already on disk, so the summary says so plainly.
///
/// systemd reads generator configuration when `zram-setup@.service` is started, which
/// normally happens at boot; qtools does not start it itself, because doing so would swap
/// out pages under a session that is already running. The summary tells the person the
/// setting takes effect at the next boot or the next start of that service.
pub fn zram_swap() -> Tweak {
    Tweak {
        id: "zram-swap",
        group: Group::Hardware,
        options: Vec::new(),
        steps: vec![
            Step::PackageInstalled { package: "zram-generator" },
            Step::FileManaged {
                path: ZRAM_CONF,
                content: "[zram0]\nzram-size = min(ram / 2, 8192)\ncompression-algorithm = zstd\n".to_owned(),
            },
        ],
        applies_to: always,
    }
}

const ZRAM_CONF: &str = "/etc/systemd/zram-generator.conf";

/// Bluetooth support: `bluez` and its command-line tools, with the service kept on.
///
/// The undo only turns the service off; it never removes the packages, because the packages
/// step already owns its own undo (uninstalling them) and `ServiceEnabled`'s undo is the one
/// asked for here.
pub fn bluetooth() -> Tweak {
    Tweak {
        id: "bluetooth",
        group: Group::Hardware,
        options: Vec::new(),
        steps: vec![
            Step::PackageInstalled { package: "bluez" },
            Step::PackageInstalled { package: "bluez-utils" },
            Step::ServiceEnabled { unit: "bluetooth.service" },
        ],
        applies_to: has_bluetooth_controller,
    }
}

/// Whether the kernel has ever seen a Bluetooth controller, built-in or USB.
///
/// `/sys/class/bluetooth` is created once a controller is found and stays empty otherwise;
/// enabling the service on a machine with none would just leave a daemon polling for a radio
/// that is never there.
fn has_bluetooth_controller(system: &mut dyn System) -> Availability {
    let present = system
        .run(&Cmd::new("sh", ["-c", "[ -n \"$(ls -A /sys/class/bluetooth 2>/dev/null)\" ]"]))
        .is_ok_and(|output| output.ok());
    if present { Availability::Yes } else { Availability::No("unavailable.no-bluetooth") }
}

/// Laptop power tuning through `tlp`, left at its own defaults.
///
/// There is no `FileManaged` step for `/etc/tlp.conf`: tlp's defaults already suit a laptop,
/// and rewriting that file would fight whatever the person already set by hand in it.
pub fn laptop_power() -> Tweak {
    Tweak {
        id: "laptop-power",
        group: Group::Hardware,
        options: Vec::new(),
        steps: vec![Step::PackageInstalled { package: "tlp" }, Step::ServiceEnabled { unit: "tlp.service" }],
        applies_to: fits_laptop_power,
    }
}

/// Fits only a machine with a battery, and only when nothing else already manages power.
fn fits_laptop_power(system: &mut dyn System) -> Availability {
    if let Availability::No(reason) = has_battery(system) {
        return Availability::No(reason);
    }
    no_power_profiles_daemon(system)
}

/// Whether the machine reports a battery; tlp has nothing to tune on a desktop.
fn has_battery(system: &mut dyn System) -> Availability {
    let present = system
        .run(&Cmd::new("sh", ["-c", "[ -n \"$(ls -d /sys/class/power_supply/BAT* 2>/dev/null)\" ]"]))
        .is_ok_and(|output| output.ok());
    if present { Availability::Yes } else { Availability::No("unavailable.no-battery") }
}

/// Whether `power-profiles-daemon` already owns power management.
///
/// tlp and power-profiles-daemon both tune the same kernel knobs; running both means whichever
/// wrote last wins, which is worse than either alone. This is the same step-aside rule
/// `time_sync` in `maintenance.rs` uses when another service already does the job.
fn no_power_profiles_daemon(system: &mut dyn System) -> Availability {
    let enabled = system
        .run(&Cmd::new("systemctl", ["is-enabled", "power-profiles-daemon.service"]))
        .is_ok_and(|output| output.ok());
    if enabled { Availability::No("unavailable.power-profiles-daemon") } else { Availability::Yes }
}

/// NVIDIA modesetting for Wayland, plus the suspend hand-off NVIDIA's own docs recommend.
///
/// qtools never installs the driver itself: which one is right (`nvidia`, `nvidia-open`,
/// `nvidia-lts`, `nouveau`) depends on the card and the running kernel, and the wrong choice
/// can leave the machine without a display at the next boot. The summary points to the Arch
/// wiki page instead.
///
/// The three hand-off units are only enabled, never started: each is a one-shot unit that runs
/// `nvidia-sleep.sh` when the machine suspends, hibernates or resumes, and starting one now
/// would suspend the driver under the running session. The initramfs is built again after the
/// option is written, because mkinitcpio's `modconf` hook copies `/etc/modprobe.d` into it
/// and a machine that loads the nvidia module early reads the option from there.
pub fn nvidia_wayland() -> Tweak {
    Tweak {
        id: "nvidia-wayland",
        group: Group::Hardware,
        options: Vec::new(),
        steps: vec![
            Step::FileManaged { path: NVIDIA_MODPROBE, content: "options nvidia_drm modeset=1\n".to_owned() },
            Step::UnitEnabled { unit: "nvidia-suspend.service" },
            Step::UnitEnabled { unit: "nvidia-resume.service" },
            Step::UnitEnabled { unit: "nvidia-hibernate.service" },
            rebuild_initramfs_after(Some(NVIDIA_MODPROBE)),
        ],
        applies_to: fits_nvidia_wayland,
    }
}

const NVIDIA_MODPROBE: &str = "/etc/modprobe.d/50-quvyta-nvidia.conf";

/// Fits only a machine with an NVIDIA card whose driver is already installed.
fn fits_nvidia_wayland(system: &mut dyn System) -> Availability {
    if !has_nvidia_card(system) {
        return Availability::No("unavailable.no-nvidia");
    }
    if !has_nvidia_driver(system) {
        return Availability::No("unavailable.no-nvidia-driver");
    }
    Availability::Yes
}

/// Whether `lspci` reports an NVIDIA display controller.
fn has_nvidia_card(system: &mut dyn System) -> bool {
    system
        .run(&Cmd::new("sh", ["-c", "lspci | grep -Ei 'vga|3d controller' | grep -qi nvidia"]))
        .is_ok_and(|output| output.ok())
}

/// Whether an NVIDIA driver package is installed, whichever build it is.
///
/// Arch names the driver by its build: the open kernel modules (`nvidia-open`), the LTS
/// kernel's (`nvidia-open-lts`, `nvidia-lts`), a DKMS build (`nvidia-open-dkms`, `nvidia-dkms`),
/// and the older cards' series from the AUR (`nvidia-580xx-dkms`, `nvidia-470xx-dkms`).
/// Modesetting is set the same way for all of them, so the list of installed packages is read
/// once and every such name counts; a fixed list of names would hide the item from people whose
/// driver works.
fn has_nvidia_driver(system: &mut dyn System) -> bool {
    system
        .run(&Cmd::new("pacman", ["-Qq"]))
        .is_ok_and(|output| output.ok() && output.stdout.lines().any(|name| is_nvidia_driver(name.trim())))
}

/// Whether a package name is one of the NVIDIA driver's builds: `nvidia` followed only by the
/// parts a build is named with, so `nvidia-utils` or `nvidia-settings` is not one.
fn is_nvidia_driver(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("nvidia") else { return false };
    rest.split('-').skip(1).all(|part| {
        matches!(part, "open" | "lts" | "dkms")
            || part
                .strip_suffix("xx")
                .is_some_and(|series| !series.is_empty() && series.bytes().all(|b| b.is_ascii_digit()))
    }) && (rest.is_empty() || rest.starts_with('-'))
}
