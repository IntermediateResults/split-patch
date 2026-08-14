build:
	cargo $(OUR_CARGO_FLAGS) build --release

fmt:
	( cd patchparser && cargo fmt )
	cargo fmt

check_formatting: fmt
	git diff --exit-code

cargo_test__patchparser:
	cd patchparser && cargo $(OUR_CARGO_FLAGS) test

cargo_test__:
	cargo $(OUR_CARGO_FLAGS) test

cargo_test: cargo_test__ cargo_test__patchparser

cargo_check:
	( cd patchparser && cargo $(OUR_CARGO_FLAGS) test --color=always )
	cargo $(OUR_CARGO_FLAGS) test --color=always

# This target is to abstract running clippy on everything (and can be run manually)
clippy:
	( cd patchparser && cargo clippy --color=always --all-targets --all-features $(CLIPPY_ARGS) )
	cargo clippy --color=always --all-targets --all-features $(CLIPPY_ARGS)

# This is for use in CI.
# Setting `RUSTFLAGS` to the same as in the `test_deny_warnings`
# target purely so that it can share the compiled binaries (i.e. save
# on compilation time).
clippy_deny:
	RUSTFLAGS="--deny warnings" CLIPPY_ARGS="-- -D warnings" make clippy

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

# Setting `CARGO_TARGET_DIR` to allow the nightly targets to compile
# fully in parallel to the others


leak_cargo_test__:
	CARGO_TARGET_DIR=target/nightly RUSTFLAGS="-Z sanitizer=leak" OUR_CARGO_FLAGS=+nightly \
		make cargo_test__
leak_cargo_test__patchparser:
	CARGO_TARGET_DIR=target/nightly RUSTFLAGS="-Z sanitizer=leak" OUR_CARGO_FLAGS=+nightly \
		make cargo_test__patchparser

leak_cargo_test: leak_cargo_test__ leak_cargo_test__patchparser

leak_test_integration:
	CARGO_TARGET_DIR=target/nightly RUSTFLAGS="-Z sanitizer=leak" OUR_CARGO_FLAGS=+nightly \
		make test_integration

leak_test: leak_cargo_test leak_test_integration

# This is for use in CI

test_deny_warnings__patchparser:
	RUSTFLAGS="--deny warnings" make cargo_test__patchparser

test_deny_warnings__:
	RUSTFLAGS="--deny warnings" make cargo_test__

miri_test:
	cargo +nightly miri test --target powerpc-unknown-linux-gnu

miri_run:
	SPLIT_PATCH=test/miri-split-patch test/run-test-for-input-dir test/div

miri: miri_test miri_run

# Run in Github CI
ci: log-timestamp
	test/ci-make \
		check_formatting \
		clippy_deny \
		test_deny_warnings__ test_deny_warnings__patchparser \
		leak_cargo_test__ leak_cargo_test__patchparser \
		leak_test_integration

log-timestamp: src/bin/log-timestamp.rs
	rustc $< -o $@

