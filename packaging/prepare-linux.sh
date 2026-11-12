
#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
destination=$1
mkdir -p -- "$destination"
destination=$(CDPATH= cd -- "$destination" && pwd)
version=$(tr -d '\r\n' < "$root/VERSION")
name="luahotkeys-$version"

temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT
mkdir -p -- "$temporary/$name"
tar -C "$root" -cf "$temporary/sources.tar" \
    Cargo.toml Cargo.lock VERSION build.rs README.md src packaging
tar -C "$temporary/$name" -xf "$temporary/sources.tar"

(
    cd "$temporary/$name"
    mkdir .cargo
    cargo vendor --locked --versioned-dirs vendor > .cargo/config.toml
    sh packaging/linux-notices.sh > packaging/THIRD-PARTY-LINUX.txt
)

archive="$name.tar.gz"
tar -C "$temporary" -czf "$destination/$archive" "$name"
checksum=$(sha256sum "$destination/$archive" | cut -d ' ' -f 1)
sed -e "s/@VERSION@/$version/g" -e "s/@SHA256@/$checksum/g" \
    "$root/packaging/arch/PKGBUILD.in" > "$destination/PKGBUILD"
sed -e "s/@VERSION@/$version/g" \
    "$root/packaging/opensuse/luahotkeys.spec.in" > "$destination/luahotkeys.spec"
printf '%s\n' "$destination/$archive"
