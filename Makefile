build:
	( cd split-patch && cargo $(OUR_CARGO_FLAGS) build --release )

fmt:
	( cd patchparser && cargo fmt )
	( cd split-patch && cargo fmt )

check_formatting: fmt
	git diff --exit-code

cargo_test:
	@echo "++ Run cargo $(OUR_CARGO_FLAGS) test on both crates"
	( cd patchparser && cargo $(OUR_CARGO_FLAGS) test --color=always)
	( cd patchparser && cargo $(OUR_CARGO_FLAGS) test --examples --color=always )
	( cd split-patch && cargo $(OUR_CARGO_FLAGS) test --color=always)

# This target is to abstract running clippy on everything (and can be run manually)
clippy:
	( cd patchparser && cargo clippy --color=always --all-targets --all-features $(CLIPPY_ARGS) )
	( cd patchparser && cargo clippy --color=always --all-targets --all-features --examples $(CLIPPY_ARGS) )
	( cd split-patch && cargo clippy --color=always --all-targets --all-features $(CLIPPY_ARGS) )

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
	@echo "++ Run tests on split-patch/test/div"
	( cd split-patch && \
		OUR_CARGO_BUILD_FLAGS="" test/run-test-for-input-dir test/div )
	@echo "++ Run tests on split-patch/test/chj-home"
	( cd split-patch && \
		OUR_CARGO_BUILD_FLAGS="" test/run-test-for-input-dir test/chj-home )

test_integration_opt:
	@echo "++ Run tests on split-patch/test/div"
	( cd split-patch && \
		OUR_CARGO_BUILD_FLAGS="--release" test/run-test-for-input-dir test/div )
	@echo "++ Run tests on split-patch/test/chj-home"
	( cd split-patch && \
		OUR_CARGO_BUILD_FLAGS="--release" test/run-test-for-input-dir test/chj-home )

test: cargo_test test_integration

# Setting `CARGO_TARGET_DIR` to allow this target to compile fully in
# parallel to the others
leak_test:
	CARGO_TARGET_DIR=target/nightly RUSTFLAGS="-Z sanitizer=leak" OUR_CARGO_FLAGS=+nightly make test

# This is for use in CI
test_deny_warnings:
	RUSTFLAGS="--deny warnings" make cargo_test

miri_test:
	( cd split-patch && cargo +nightly miri test --target powerpc-unknown-linux-gnu )

miri_run:
	( cd split-patch && \
		SPLIT_PATCH=test/miri-split-patch OUR_CARGO_BUILD_FLAGS="" test/run-test-for-input-dir test/div )

miri: miri_test miri_run

# Run in Github CI
ci: test/target/debug/log-timestamp
	test/ci-make test_deny_warnings clippy_deny leak_test check_formatting

verbose_ci:
	CI_MAKE_VERBOSE=1 make ci

# Build test utility the fastest way possible
test/target/debug/log-timestamp: test/src/bin/log-timestamp.rs
	mkdir -p test/target/debug/
	rustc $< -o $@

