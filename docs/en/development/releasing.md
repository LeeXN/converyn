# Release a version

[English](releasing.md) | [简体中文](../../zh-CN/development/releasing.md)

`main` is the ongoing development branch. Feature branches and an optional `dev` branch run the same verification workflow; merge completed changes into `main`. Branch pushes do not publish packages. An annotated `vX.Y.Z` tag selects the exact source revision to release.

## Prepare the version

The product version in `Cargo.toml` is authoritative. All Rust workspace packages inherit it. Keep `web/package.json`, both root version fields in `web/package-lock.json`, `Cargo.lock` and the generated OpenAPI version equal. Plugin identities, storage schema and communication protocol have independent versions.

After editing the product versions in `Cargo.toml` and `web/package.json`, regenerate the lockfiles and API:

```bash
cargo check --workspace --all-features
npm install --package-lock-only --ignore-scripts --prefix web
python3 tools/generate_api.py
python3 tools/render_api_docs.py
python3 tools/check_version.py
```

Run the [development checks](testing.md), commit all completed changes and the version update, push `main`, and wait for the Verify workflow to pass. Use patch releases for compatible fixes and minor releases for feature or contract changes while the product is below its first stable major version. Describe breaking changes and required upgrade steps in the release notes.

## Publish the tag

Run from the reviewed, clean `main` checkout. The version checker returns the product version so the tag cannot silently select a different version:

```bash
release_version=$(python3 tools/check_version.py)
git tag -a "v$release_version" -m "Release $release_version"
git push origin "v$release_version"
```

The Release workflow checks the tag against all product versions, calls the full Verify workflow, then builds native amd64 and arm64 static musl binaries with the pinned Rust toolchain and full plugin set. It checks binary versions, static ELF format, public-content boundaries and manifest hashes before packaging.

Both architectures must succeed. CI creates a draft Release, uploads both archives and `SHA256SUMS`, then publishes it with generated notes. A stable release becomes the latest release; a tag containing a prerelease identifier stays a prerelease. Only the final publishing job has repository-content write permission; no personal access token is required.

## Download and verify

The [GitHub Releases page](https://github.com/LeeXN/converyn/releases) contains:

| Asset | Contents |
| --- | --- |
| `converyn-VERSION-linux-amd64.tar.gz` | Static amd64 binary, installer, initializer, manifest and license |
| `converyn-VERSION-linux-arm64.tar.gz` | Static arm64 binary, installer, initializer, manifest and license |
| `SHA256SUMS` | SHA-256 checksums of both archives |

Download both archives to check the complete checksum file, or select the checksum line for the archive you downloaded. Extract the archive for the target architecture. It contains `converyn-linux-amd64` or `converyn-linux-arm64`, `install.sh`, `init.sh`, `manifest.json` and `LICENSE`. Keep the binary and scripts from the same archive together. Follow the [installation tutorial](../tutorials/quickstart.md).

## Retry a failed release

A failed verification or build does not publish a Release. Fix the problem on the development branch. If the tagged source is correct and the failure was transient, rerun the failed workflow or dispatch Release with the existing tag as its ref:

```bash
gh workflow run release.yml --ref "v$release_version"
```

Retries may complete an existing draft and replace its partial draft assets. Published releases and their tags are not overwritten; publish corrections under a new version. Never move an already published tag or reuse its version for different source. Inspect the workflow run and archive manifests when diagnosing a failed build. The [release builder](../../../tools/build_release.py) can also build packages locally; see [contributing](../../../CONTRIBUTING.md).
