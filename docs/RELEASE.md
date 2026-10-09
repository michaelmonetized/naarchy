# Local desktop releases

Naarchy's Rust builds, tests, and packages run locally. GitHub hosts the source
and downloadable release assets; it does not build this desktop application.
Blacksmith is for website work only. Do not enable GitHub-hosted build workflows,
pay for hosted compute/cache, or change billing or repository protection settings
to publish. A failed or unavailable hosted job is not local validation.

## Validate and build

Use the checks in [CONTRIBUTING.md](../CONTRIBUTING.md), temporary XDG state,
and the seasonal native fixture for GTK overlay changes. Build both supported
targets locally with the declared Rust minimum:

```bash
cargo build --locked --release --bins --target aarch64-unknown-linux-gnu
cargo build --locked --release --bins --target x86_64-unknown-linux-gnu
```

Cross builds require the corresponding native libraries and linker. Run the
test suite and CLI/installer smoke checks on compatible local runtimes where
available. Record architecture, toolchain, runtime, and any untested physical
or multi-monitor coverage. Headless rendering is software evidence, not a
physical desktop or performance claim.

## Package the exact source

Commit reviewed changes, reconcile current main, and merge using the exact
reviewed head without bypassing actual repository protections. Confirm the
merged source tree matches the locally built source. Rebuild if application
source changed. Set the Cargo version and PKGBUILD version consistently.

```bash
bash scripts/package-release.sh \
  /absolute/path/to/arm64/naarchy \
  /absolute/path/to/x86_64/naarchy \
  /absolute/path/to/dist FULL_MERGE_COMMIT
cd /absolute/path/to/dist
sha256sum -c SHA256SUMS
```

The helper packages the exact committed documentation and source, validates ELF
architecture and the documented glibc 2.39 ceiling, and includes commit/tree and
binary hashes in each binary archive's `BUILDINFO.json`. It does not install,
push, or publish. Extract both archives to fresh temporary directories, use
`DESTDIR` with the included installer, and check binary/icon/launcher/unit files.

## Publish reviewed local files

Confirm the intended tag does not already exist and matches Cargo's version.
Create an annotated tag on the verified merge commit, push that tag, and create
a draft GitHub release with the two local binary archives, versioned source
archive, and `SHA256SUMS`. Review the release notes and checksums, then publish
the draft using the authorized release account. No hosted build is involved.

After publishing, independently download all assets from their public URLs,
verify `SHA256SUMS`, inspect archive provenance/version, and confirm the exact
remote tag and main commit. Check the public latest-release installation path
before reporting shipment. Keep the running app unchanged unless installation
was separately approved.
