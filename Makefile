build:
	cargo $(OUR_CARGO_FLAGS) build --release

fmt:
	( cd patchparser && cargo fmt )
	cargo fmt

check_formatting: fmt
	git diff --exit-code

cargo_test:
	@echo "++ Run cargo $(OUR_CARGO_FLAGS) test on both crates"
	( cd patchparser && cargo $(OUR_CARGO_FLAGS) test )
	cargo $(OUR_CARGO_FLAGS) test

cargo_check:
	( cd patchparser && cargo $(OUR_CARGO_FLAGS) test --color=always )
	cargo $(OUR_CARGO_FLAGS) test --color=always

# This target is to abstract running clippy on everything (and can be run manually)
clippy:
	( cd patchparser && cargo clippy --color=always --all-targets --all-features $(CLIPPY_ARGS) )
	cargo clippy --color=always --all-targets --all-features $(CLIPPY_ARGS)

# This is for use in CI
clippy_deny:
	CLIPPY_ARGS="-- -D warnings" make clippy

# This is for manual use
clippy_fix:
	CLIPPY_ARGS="--fix" make clippy

test_integration:
	@echo "++ Run tests on test/div"
	OUR_CARGO_BUILD_FLAGS="" test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	OUR_CARGO_BUILD_FLAGS="" test/run-test-for-input-dir test/chj-home

test_integration_opt:
	@echo "++ Run tests on test/div"
	OUR_CARGO_BUILD_FLAGS="--release" test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	OUR_CARGO_BUILD_FLAGS="--release" test/run-test-for-input-dir test/chj-home

test: cargo_test test_integration

leak_test:
	RUSTFLAGS="-Z sanitizer=leak" OUR_CARGO_FLAGS=+nightly make test

# This is for use in CI
test_deny_warnings: clippy_deny
	RUSTFLAGS="--deny warnings" make cargo_test

miri_test:
	cargo +nightly miri test --target powerpc-unknown-linux-gnu

miri_run:
	SPLIT_PATCH=test/miri-split-patch test/run-test-for-input-dir test/div

miri: miri_test miri_run
