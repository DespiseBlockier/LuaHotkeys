
# LuaHotkeys on Linux

The Linux build is a terminal runner for an X11 desktop session. It runs the same Lua 5.1 macros and Luau-style type annotations as the Windows app. Use a text editor of your choice to edit a script.

```sh
luahotkeys --keys
luahotkeys --run ./my-script.lhk
```

Keep the runner open for global shortcuts. Press Ctrl+C in the terminal to stop it and release its shortcuts. A script without hotkeys exits when it finishes. Terminal output contains the messages from `log` and `print`.

`hotkey`, `press`, `type_text`, `click`, `move_mouse`, and `mouse_pos` use X11. Type and pointer actions use `xdotool`. When the desktop is running Wayland, select an X11 session at the login screen to use those functions. Keyboard layouts, desktop shortcuts, and available keysyms affect which combinations can bind. `run` launches a Linux program with separate arguments or opens a local document or URL through `xdg-open`. Replace Windows executable names in scripts with Linux equivalents.

## Build the runner

Build with Rust 1.88 or newer and a C compiler. The runner builds Lua 5.1 from source as part of Cargo. Install `xdotool` and `xdg-utils` for the Linux desktop functions.

```sh
cargo build --locked --release --bin luahotkeys
./target/release/luahotkeys --run packaging/example.lhk
```

## Build installable packages

From a Linux checkout, run the following once to produce a checksummed source archive and the two package recipes. This step fetches the locked Cargo sources. Package builds then use those sources offline.

```sh
sh packaging/prepare-linux.sh dist/linux-packaging
cd dist/linux-packaging
```

### Arch Linux (x86-64)

Install `base-devel` and run `makepkg` as a regular user. From the packaging directory:

```sh
makepkg -si
```

If you already have the package file, install it with `sudo pacman -U ./luahotkeys-<version>-1-x86_64.pkg.tar.zst`.

An AUR listing would host the `PKGBUILD` and `.SRCINFO`. Point the recipe at a permanent HTTPS URL for the release source archive, keep its checksum, and generate `.SRCINFO` with `makepkg --printsrcinfo`. An AUR entry is a build recipe. For `pacman -S luahotkeys`, publish the signed package and a `repo-add` database on your own pacman repository, then have users add that repository to `pacman.conf`. [Arch makepkg](https://man.archlinux.org/man/makepkg.8) · [repo-add](https://man.archlinux.org/man/repo-add.8)

### openSUSE Tumbleweed (x86-64)

With `rpm-build`, `cargo`, `rust`, and `gcc` installed, build from the packaging directory:

```sh
rpmbuild -bb --define "_sourcedir $PWD" luahotkeys.spec
```

Install the resulting RPM with `sudo zypper install ./luahotkeys-<version>-<release>.x86_64.rpm`. Copy it from the `Wrote:` path that `rpmbuild` prints, or pass that absolute path to Zypper.

For a hosted repository, create a package in Open Build Service with the spec and vendored source archive, add the Tumbleweed target, and publish the build. Users add the generated `.repo` URL with `sudo zypper addrepo -f <repo-url> luahotkeys` and run `sudo zypper install luahotkeys`. Build a separate package for each additional openSUSE release you intend to support. [OBS beginner’s guide](https://openbuildservice.org/help/manuals/obs-user-guide/art-obs-bg)

### Flatpak

A self-hosted Flatpak can bundle the runner and `xdotool` with X11 access. After building it into your own Flatpak repository, publish a signed remote for `flatpak remote-add` and `flatpak install`, or distribute a `.flatpak` file that users can install with `flatpak install --user ./LuaHotkeys.flatpak`. Test script-directory access and program launching from the sandbox before shipping. The native Arch and openSUSE packages are the current install paths. [Flatpak hosting](https://docs.flatpak.org/en/latest/hosting-a-repository.html) · [Single-file bundles](https://docs.flatpak.org/en/latest/single-file-bundles.html)

Flathub generally requires a graphical app and has separate submission rules, including restrictions on AI-assisted manifests. A public submission also needs clear redistribution terms from the project owner. [Flathub requirements](https://docs.flathub.org/docs/for-app-authors/requirements)
