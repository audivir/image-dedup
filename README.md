# image-dedup

A terminal UI for finding duplicate or similar images and videos, and for sorting media into
folders by date. It is built on top of `czkawka_core`, the scanning engine behind the
[Czkawka](https://github.com/qarmin/czkawka) project.

## Prerequisites

- The Rust toolchain (`cargo`, `rustc`), edition 2024.
- `git`, for cloning with submodules.
- Optional: `libheif` development headers, if building with the `heif` feature.

## Installation

```shell
git clone --recurse-submodules git@github.com:audivir/image-dedup
cd image-dedup
cd vendor/czkawka && git apply ../../czkawka.patch && cd ../..
cargo build --release
```

If the repository was cloned without `--recurse-submodules`, run
`git submodule update --init` first.

## Usage

```shell
image_dedup --directories /path/to/photos
```

Pass `--sort-mode` to sort media by date into folders instead of deduplicating:

```shell
image_dedup --sort-mode --directories /path/to/photos
```

Run `image_dedup --help` for the full list of options, including hash algorithm, similarity
thresholds, thread count, and file filtering.

## Acknowledgments

This project is built on `czkawka_core` from [Czkawka](https://github.com/qarmin/czkawka) by
qarmin and contributors, vendored under `vendor/czkawka` with local patches from `czkawka.patch`.
See `NOTICE` for license details.

## License

MIT, see `LICENSE`.
