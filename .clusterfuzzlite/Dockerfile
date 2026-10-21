# Build image for ClusterFuzzLite (https://google.github.io/clusterfuzzlite/).
#
# The base image is OSS-Fuzz's Rust builder: a nightly Rust, `cargo fuzz`, and
# the clang and libFuzzer the sanitizer builds link against. The fuzz targets
# need nothing else, so the image only copies the repository in. `build.sh`
# builds every target and its seed corpus into $OUT.
FROM gcr.io/oss-fuzz-base/base-builder-rust
COPY . $SRC/inference
WORKDIR $SRC/inference
COPY .clusterfuzzlite/build.sh $SRC/
