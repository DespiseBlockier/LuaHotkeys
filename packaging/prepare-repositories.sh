#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
    printf '%s\n' 'Usage: sh prepare-repositories.sh ARCH_PACKAGE RPM OUTPUT_DIRECTORY' >&2
    exit 2
fi

directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fingerprint=$(sed -n 's/^fingerprint=//p' "$directory/install-linux.sh")
case "$fingerprint" in
    *[!0-9A-F]* | '') printf '%s\n' 'Invalid repository signing fingerprint.' >&2; exit 1 ;;
esac
[ "${#fingerprint}" -eq 40 ]

arch_package=$1
rpm_package=$2
destination=$3
if [ -e "$destination" ]; then
    printf '%s\n' 'Choose a new output directory.' >&2
    exit 1
fi

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
public="$scratch/public"
arch="$public/arch/x86_64"
suse="$public/opensuse/tumbleweed/x86_64"
mkdir -p "$arch" "$suse" "$scratch/rpmdb"
gpg --armor --export "$fingerprint" > "$public/luahotkeys.asc"
test -s "$public/luahotkeys.asc"

arch_file="$arch/$(basename -- "$arch_package")"
cp -- "$arch_package" "$arch_file"
gpg --local-user "$fingerprint" --detach-sign "$arch_file"
repo-add --include-sigs "$scratch/luahotkeys.db.tar.gz" "$arch_file"
for index in db files; do
    cp "$scratch/luahotkeys.$index.tar.gz" "$arch/luahotkeys.$index"
    gpg --local-user "$fingerprint" --detach-sign "$arch/luahotkeys.$index"
done

rpm_file="$suse/$(basename -- "$rpm_package")"
cp -- "$rpm_package" "$rpm_file"
rpmsign --define '_openpgp_sign gpg' --define "_gpg_name $fingerprint" \
    --define "_openpgp_sign_id $fingerprint" --addsign "$rpm_file"
rpmkeys --dbpath "$scratch/rpmdb" --import "$public/luahotkeys.asc"
rpmkeys --dbpath "$scratch/rpmdb" --checksig "$rpm_file"
createrepo_c --compress-type gz "$suse"
cp "$public/luahotkeys.asc" "$suse/repodata/repomd.xml.key"
gpg --local-user "$fingerprint" --armor --detach-sign "$suse/repodata/repomd.xml"

cp "$directory/install-linux.sh" "$public/install-linux.sh"
touch "$public/.nojekyll"
mv -- "$public" "$destination"
