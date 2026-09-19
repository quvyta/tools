# Contributing to qtools

Thank you for taking the time. Bug reports, ideas and pull requests are all welcome.

## Reporting a bug or asking for something

Open an issue at <https://github.com/quvyta/tools/issues> and pick the template that fits. For a bug, the item involved, its state on the screen and your distribution usually decide where the problem is, so the template asks for them. Please check that nothing private (host names, user names, keys, addresses) shows in what you paste before you attach it.

A setting you would like qtools to offer is welcome as a feature request: say what you set up by hand today, the files and services it touches, and how you would undo it.

## Building and testing

The toolchain is pinned by `rust-toolchain.toml`; `rustup` picks it up by itself.

```sh
git clone https://github.com/quvyta/tools
cd tools
git config core.hooksPath .githooks   # once: formatting, clippy, tests and docs before every commit
cargo test
```

The tests never change the machine they run on. Every command, file and service goes through one layer (`src/system.rs`) that the tests replace with a double and a temporary folder, so `cargo test` is safe anywhere and needs no `sudo`.

To try the screen, run `cargo run --bin qtools`. It only reads the machine until you confirm an item; `enter`, `--run` and `--revert` change your system for real, so try them on a machine you can afford to repair, such as a virtual machine or a container.

## Pull requests

- Keep one change per pull request, and say in the description what it changes for the person using qtools.
- The commit hook must pass: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` and `cargo doc` without warnings. Please do not skip it.
- A bug fix comes with a test that fails without it.
- A new item must show everything it touches before it runs, keep a backup of every file it writes, be undoable, and change nothing when it is run a second time. It needs tests for all four.
- Text the person reads lives in `assets/locales/`, never in the code. Add the English line; if you do not speak the other languages, say so in the pull request and leave them to a later change.
- qtools never runs as root and never handles a password: steps that need privileges go through `sudo` in the embedded terminal. A change that would break that will not be merged.
- The interface comes from [quvyta-framework](https://github.com/quvyta/framework). A widget or behaviour every Quvyta application would need belongs there; open an issue in that repository first.

## Licence

By contributing you agree that your contribution is licensed under the MIT licence of this repository.
