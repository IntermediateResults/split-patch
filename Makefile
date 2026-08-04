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

test_integration:
	cargo $(OUR_CARGO_FLAGS) build --quiet
	@echo "++ Run tests on test/div"
	test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	test/run-test-for-input-dir test/chj-home

test_integration_opt:
	cargo $(OUR_CARGO_FLAGS) build --quiet --release
	@echo "++ Run tests on test/div"
	SPLIT_PATCH=target/release/split-patch test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	SPLIT_PATCH=target/release/split-patch test/run-test-for-input-dir test/chj-home

test: cargo_test test_integration

leak_test:
	RUSTFLAGS="-Z sanitizer=leak" OUR_CARGO_FLAGS=+nightly make test

test_deny_warnings:
	RUSTFLAGS="--deny warnings" make cargo_test

miri_test:
	cargo +nightly miri test --target powerpc-unknown-linux-gnu

miri_run:
	SPLIT_PATCH=test/miri-split-patch test/run-test-for-input-dir test/div

miri: miri_test miri_run
