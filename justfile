set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

# Format Rust and TOML; select nightly rustfmt externally (see README).
fmt:
    cargo fmt --all
    taplo format

# Check formatting without writing.
fmt-check:
    cargo fmt --all -- --check
    taplo format --check

# Lint the whole workspace; warnings are errors.
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run unit, integration, example, and doctests.
test:
    cargo test --workspace --all-features

# Verify the checked-in HUT catalog against spec/source.toml.
verify:
    cargo xtask verify

# no_std / thumb checks for firmware crates.
embedded:
    cargo check -p hidder-core --target thumbv7em-none-eabihf
    cargo check -p hid-usage-tables --target thumbv7em-none-eabihf
    cargo check -p hidder-dsl --target thumbv7em-none-eabihf
    cargo check -p hidder-parser --target thumbv7em-none-eabihf
    cargo check -p hidder --no-default-features --target thumbv7em-none-eabihf

# Build rustdoc with warnings denied.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

# Regenerated HID Usage Tables catalog.
generate:
    cargo xtask generate

# Fail if generated.rs is stale.
generate-check:
    cargo xtask generate --check

# Print catalog statistics.
stats:
    cargo xtask stats

# Run the facade examples.
examples:
    cargo run -p hidder --example mouse
    cargo run -p hidder --example parse --features parser
    cargo run -p hidder-core --example roundtrip
    cargo run -p hid-usage-tables --example classify
    cargo run -p hidder-dsl --example joystick
    cargo run -p hidder-macros --example consumer
    cargo run -p hidder-parser --example numbered
    cargo run -p hidder-parser --example stores

# Full local CI gate, including rustdoc with warnings denied.
ci: fmt-check clippy test verify embedded doc
