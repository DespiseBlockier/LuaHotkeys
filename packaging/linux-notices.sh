
#!/bin/sh
set -eu

printf '%s\n\n' 'LuaHotkeys third-party notices'
printf '%s\n' \
    'The following parser sources are available under the Mozilla Public License 2.0.' \
    'License: https://www.mozilla.org/MPL/2.0/' \
    'Full Moon: https://crates.io/api/v1/crates/full_moon/2.2.0/download' \
    'Full Moon derive: https://crates.io/api/v1/crates/full_moon_derive/0.11.0/download'
printf '\n%s\n' 'License files from the locked Cargo sources follow.'
printf '\n%s\n\n' 'Merriweather Regular (SIL Open Font License 1.1)'
cat src/macro_gui/immediate/Merriweather-OFL.txt
find vendor -type f \( -iname 'license*' -o -iname 'copying*' -o -iname 'notice*' -o -iname 'copyright*' \) \
    | LC_ALL=C sort \
    | while IFS= read -r file; do
        printf '\n%s\n\n' "$file"
        cat "$file"
    done
