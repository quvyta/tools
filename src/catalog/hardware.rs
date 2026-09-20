//! Microcode, graphics, power and swap.

use crate::step::Step;
use crate::system::{Cmd, System};
use crate::tweak::{Availability, Group, Tweak};

/// Always fits: every machine can use more usable memory.
fn always(_: &mut dyn System) -> Availability {
    Availability::Yes
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
/// wiki page instead. qtools also does not touch the initramfs: if the nvidia module is built
/// into it, `mkinitcpio -P` is still needed by hand, and the summary says so.
pub fn nvidia_wayland() -> Tweak {
    Tweak {
        id: "nvidia-wayland",
        group: Group::Hardware,
        options: Vec::new(),
        steps: vec![
            Step::FileManaged { path: NVIDIA_MODPROBE, content: "options nvidia_drm modeset=1\n".to_owned() },
            Step::ServiceEnabled { unit: "nvidia-suspend.service" },
            Step::ServiceEnabled { unit: "nvidia-resume.service" },
            Step::ServiceEnabled { unit: "nvidia-hibernate.service" },
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

/// Whether any of the NVIDIA driver packages is installed.
///
/// All five are in the list because a machine may run the open kernel modules
/// (`nvidia-open`), the LTS kernel's build (`nvidia-lts`) or a DKMS build, and modesetting is
/// set the same way for every one of them; asking only for `nvidia` would hide the tweak from
/// people who already have a working driver.
fn has_nvidia_driver(system: &mut dyn System) -> bool {
    ["nvidia", "nvidia-open", "nvidia-lts", "nvidia-dkms", "nvidia-open-dkms"]
        .iter()
        .any(|package| system.run(&Cmd::new("pacman", ["-Q", package])).is_ok_and(|output| output.ok()))
}
