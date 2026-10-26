#!/usr/bin/env bash
# Builds the Goose compiler Ferrous Goose is tested against: the pinned commit
# below with the patch series in goose-patches/ applied (docs/goose-patches.md).
#
#   scripts/toolchain.sh            build into .toolchain/goose/build/goose
#   scripts/toolchain.sh --stock    the pinned commit without the patches,
#                                   into .toolchain/goose-stock/build/goose
#
# GOOSE_REPO names where to clone from (default: GitHub). A local clone works
# too, GOOSE_REPO=~/github/goose, as long as it has the pinned commit.
set -euo pipefail

GOOSE_COMMIT=a108677d300cb0e8bbb842c35a353abe67a20c6c
GOOSE_REPO=${GOOSE_REPO:-https://github.com/aardappel/goose.git}

root=$(cd "$(dirname "$0")/.." && pwd)
stock=0
if [[ "${1:-}" == "--stock" ]]; then stock=1; fi
if [[ $stock == 1 ]]; then dir="$root/.toolchain/goose-stock"; else dir="$root/.toolchain/goose"; fi

if [[ ! -d "$dir/.git" ]]; then
    mkdir -p "$(dirname "$dir")"
    git clone --no-checkout "$GOOSE_REPO" "$dir"
fi
if ! git -C "$dir" cat-file -e "$GOOSE_COMMIT^{commit}" 2>/dev/null; then
    git -C "$dir" fetch "$GOOSE_REPO" "+refs/heads/*:refs/remotes/origin/*" "+refs/remotes/origin/*:refs/remotes/origin/*" ||
        git -C "$dir" fetch "$GOOSE_REPO"
fi

# The patched tree is rebuilt from the pinned commit whenever the series
# changes, recorded by its hash.
series=""
if [[ $stock == 0 ]]; then
    series=$(cat "$root"/goose-patches/*.patch 2>/dev/null | git hash-object --stdin | cut -c1-12)
fi
stamp="$dir/.native-series"
if [[ ! -f "$stamp" || "$(cat "$stamp")" != "$GOOSE_COMMIT $series" ]]; then
    git -C "$dir" checkout -q -f --detach "$GOOSE_COMMIT"
    git -C "$dir" clean -q -fdx -e build
    if [[ $stock == 0 ]]; then
        for p in "$root"/goose-patches/*.patch; do
            [[ -e "$p" ]] || continue
            echo "applying $(basename "$p")"
            git -C "$dir" apply --whitespace=nowarn "$p"
        done
    fi
    echo "$GOOSE_COMMIT $series" > "$stamp"
fi

cmake_cmd=(cmake)
if ! command -v cmake >/dev/null 2>&1; then
    if command -v nix >/dev/null 2>&1; then
        cmake_cmd=(nix shell nixpkgs#cmake -c cmake)
    else
        echo "toolchain.sh: cmake is needed to build Goose" >&2
        exit 1
    fi
fi
gen=()
# Ninja where the C++ compiler is on PATH too (on Windows, a Visual Studio
# developer shell); CMake's default generator otherwise.
if command -v ninja >/dev/null 2>&1 && { [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* ]] || command -v cl >/dev/null 2>&1; }; then
    gen=(-G Ninja)
fi

"${cmake_cmd[@]}" -S "$dir" -B "$dir/build" ${gen[@]+"${gen[@]}"} -DCMAKE_BUILD_TYPE=Release -DGOOSE_BUILTIN_LIBS= >/dev/null
"${cmake_cmd[@]}" --build "$dir/build" --config Release --target goose
# The Visual Studio generator puts it under Release/; the rest of the tree
# looks for build/goose.
if [[ -f "$dir/build/Release/goose.exe" ]]; then cp "$dir/build/Release/goose.exe" "$dir/build/goose.exe"; fi
echo "$dir/build/goose"
