#!/bin/sh
set -eu

site=https://despiseblockier.github.io/LuaHotkeys
fingerprint=65E020859EBA2286077FFC5CF249A7585BC995B5

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    else
        sudo "$@"
    fi
}

case "${1-}" in
    '') repo_only=no ;;
    --repo-only) repo_only=yes ;;
    *) fail 'Usage: sh install-linux.sh [--repo-only]' ;;
esac
[ "$#" -le 1 ] || fail 'Usage: sh install-linux.sh [--repo-only]'
[ "$(uname -m)" = x86_64 ] || fail 'LuaHotkeys packages require an x86-64 system.'
[ -r /etc/os-release ] || fail 'Could not identify this Linux distribution.'
. /etc/os-release
case "${ID-}" in
    arch | opensuse-tumbleweed) ;;
    *) fail 'This installer supports Arch Linux and openSUSE Tumbleweed.' ;;
esac

command -v curl >/dev/null || fail 'Install curl and rerun this installer.'
if command -v gpg >/dev/null; then
    gpg=gpg
elif command -v gpg2 >/dev/null; then
    gpg=gpg2
else
    fail 'Install GnuPG and rerun this installer.'
fi
if [ "$(id -u)" -ne 0 ]; then
    command -v sudo >/dev/null || fail 'Run this installer with sudo or from a root shell.'
fi

umask 077
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
key="$work/luahotkeys.asc"
mkdir "$work/gnupg"
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
    --output "$key" "$site/luahotkeys.asc"
key_details=$("$gpg" --batch --homedir "$work/gnupg" --with-colons --show-keys "$key")
primary_keys=$(printf '%s\n' "$key_details" | sed -n '/^pub:/p' | wc -l)
actual=$(printf '%s\n' "$key_details" | sed -n '/^fpr:/{s/^fpr:::::::::\([0-9A-F]\{40\}\):.*$/\1/p;q;}')
if [ "$primary_keys" -ne 1 ] || [ "$actual" != "$fingerprint" ]; then
    fail 'The repository signing key fingerprint does not match.'
fi

if [ "$ID" = arch ]; then
    server="$site/arch/\$arch"
    config=/etc/pacman.conf
    [ -f "$config" ] || fail 'Could not find /etc/pacman.conf.'
    repositories=$(pacman-conf --repo-list)
    if printf '%s\n' "$repositories" | grep -Fxq luahotkeys; then
        existing=$(pacman-conf -r luahotkeys Server)
        [ "$existing" = "$site/arch/x86_64" ] || fail 'Pacman already has a different luahotkeys repository.'
        signature_policy=$(pacman-conf -r luahotkeys SigLevel)
        printf '%s\n' "$signature_policy" | grep -Fxq PackageRequired ||
            fail 'Set SigLevel = Required TrustedOnly for the luahotkeys repository.'
        printf '%s\n' "$signature_policy" | grep -Fxq DatabaseRequired ||
            fail 'Set SigLevel = Required TrustedOnly for the luahotkeys repository.'
        if printf '%s\n' "$signature_policy" | grep -Eq '^(Package|Database)TrustAll$'; then
            fail 'Set SigLevel = Required TrustedOnly for the luahotkeys repository.'
        fi
    else
        printf '\n[luahotkeys]\nSigLevel = Required TrustedOnly\nServer = %s\n' "$server" > "$work/arch.conf"
    fi
    as_root pacman-key --init
    as_root pacman-key --add "$key"
    as_root pacman-key --lsign-key "$fingerprint"
    if [ -f "$work/arch.conf" ]; then
        as_root tee -a "$config" < "$work/arch.conf" >/dev/null
    fi

    # Refresh this repository without advancing the Arch repositories' databases.
    printf '[options]\nArchitecture = auto\n[luahotkeys]\nSigLevel = Required TrustedOnly\nServer = %s\n' \
        "$server" > "$work/pacman.conf"
    as_root pacman --config "$work/pacman.conf" -Sy
    if [ "$repo_only" = no ]; then
        as_root pacman -S --needed luahotkeys/luahotkeys
    fi
else
    config=/etc/zypp/repos.d/luahotkeys.repo
    cat > "$work/luahotkeys.repo" <<EOF
[luahotkeys]
name=LuaHotkeys
enabled=1
autorefresh=1
baseurl=$site/opensuse/tumbleweed/x86_64
type=rpm-md
gpgcheck=1
repo_gpgcheck=1
pkg_gpgcheck=1
EOF
    if [ -e "$config" ]; then
        existing=$(as_root cat "$config")
        expected=$(cat "$work/luahotkeys.repo")
        [ "$existing" = "$expected" ] || fail 'Zypper already has a different luahotkeys repository configuration.'
    fi
    as_root rpmkeys --import "$key"
    if [ ! -e "$config" ]; then
        as_root install -m 0644 "$work/luahotkeys.repo" "$config"
    fi
    as_root zypper --non-interactive refresh luahotkeys
    if [ "$repo_only" = no ]; then
        as_root zypper install --from luahotkeys luahotkeys
    fi
fi
