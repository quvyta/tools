//! The firewall and what listens on the network.

use crate::step::Step;
use crate::system::{Cmd, System};
use crate::tweak::{Availability, Group, Tweak};

/// Always fits: every machine has a network stack to guard.
fn always(_: &mut dyn System) -> Availability {
    Availability::Yes
}

/// A host firewall through ufw: installed, turned on, and kept on across boots.
///
/// There is no policy step. The `/etc/default/ufw` the package ships already sets
/// `DEFAULT_INPUT_POLICY="DROP"` and `DEFAULT_OUTPUT_POLICY="ACCEPT"`, so enabling it is
/// enough to block incoming and allow outgoing connections; rewriting that shell-style
/// file would only risk a conflict with what the person set by hand.
///
/// `ufw enable` records itself as `ENABLED=yes` in `/etc/ufw/ufw.conf`, which is
/// world-readable, so the check needs no root. `--force` skips ufw's own "may disrupt
/// existing ssh connections" prompt: qtools has already asked, and a prompt inside the
/// embedded terminal would stall the run. The service step makes the rules load at boot.
///
/// qtools opens no port. SSH and anything else that must stay reachable from outside
/// needs its own `ufw allow` rule, and the summary says so.
pub fn firewall() -> Tweak {
    Tweak {
        id: "firewall",
        group: Group::Security,
        options: Vec::new(),
        steps: vec![
            Step::PackageInstalled { package: "ufw" },
            Step::CommandRun {
                run: Cmd::new("ufw", ["--force", "enable"]).root(),
                check: Cmd::new("sh", ["-c", "grep -qx 'ENABLED=yes' /etc/ufw/ufw.conf"]),
                undo: Cmd::new("ufw", ["disable"]).root(),
                label: "ufw",
            },
            Step::ServiceEnabled { unit: "ufw.service" },
        ],
        applies_to: always,
    }
}

/// Turns off password and root logins over SSH, in a drop-in of qtools' own.
///
/// Arch's stock `/etc/ssh/sshd_config` starts with `Include /etc/ssh/sshd_config.d/*.conf`
/// on its second line, and sshd keeps the first value it reads for a keyword, so a drop-in
/// wins over the main file and over any later drop-in; `50-` sorts before the package's
/// own `99-archlinux.conf`. The stock file stays untouched and the undo is removing the
/// drop-in.
///
/// sshd reads its configuration at start, so nothing changes until it is restarted; qtools
/// does not restart it, because that could cut the very session the person is working in.
/// Once it does, password logins stop working: a key must already be in place on a machine
/// reached remotely, and the summary warns about exactly that.
pub fn ssh_hardening() -> Tweak {
    Tweak {
        id: "ssh-hardening",
        group: Group::Security,
        options: Vec::new(),
        steps: vec![Step::FileManaged {
            path: SSHD_DROP_IN,
            content: "PasswordAuthentication no\nPermitRootLogin no\n".to_owned(),
        }],
        applies_to: has_openssh,
    }
}

const SSHD_DROP_IN: &str = "/etc/ssh/sshd_config.d/50-quvyta-tools.conf";

/// Whether the SSH server is installed at all; a drop-in for a daemon that is not there
/// would only sit unread.
fn has_openssh(system: &mut dyn System) -> Availability {
    let installed = system.run(&Cmd::new("pacman", ["-Q", "openssh"])).is_ok_and(|output| output.ok());
    if installed { Availability::Yes } else { Availability::No("unavailable.no-openssh") }
}
