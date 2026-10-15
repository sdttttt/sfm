# Sfm — build and run the menu bar app.
#
# `NSStatusItem` only gets a slot in the menu bar when the process runs from a
# real bundle, so `cargo run` alone is not enough: `build` assembles Sfm.app
# and ad-hoc signs it.

app := "Sfm"
bundle := app + ".app"
binary := "target/release/sfm"

# Assemble Sfm.app from a release build.
default: build

build:
    cargo build --release
    rm -rf "{{bundle}}"
    mkdir -p "{{bundle}}/Contents/MacOS" "{{bundle}}/Contents/Resources"
    cp "{{binary}}" "{{bundle}}/Contents/MacOS/sfm"
    cp Info.plist "{{bundle}}/Contents/Info.plist"
    # Ad-hoc signature is enough for local use; no Developer ID needed.
    codesign --force --sign - --timestamp=none "{{bundle}}"
    @echo "built {{bundle}}"

# Run from the terminal rather than `open`, so panics are visible here.
run: build
    cd "{{bundle}}/Contents/MacOS" && ./sfm

# Run the app with one line per refresh on stderr.
debug: build
    cd "{{bundle}}/Contents/MacOS" && SFM_DEBUG=1 ./sfm

# Run the unit tests.
test:
    cargo test

# Check every menu action without a human clicking, and print the shown values.
selftest: build
    cd "{{bundle}}/Contents/MacOS" && ./sfm --selftest

# Remove build output.
clean:
    cargo clean
    rm -rf "{{bundle}}"
