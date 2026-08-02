build:
	cargo build --release

build_test:
	cargo build --quiet

cargo_test:
	@echo "++ Run cargo test on both crates"
	( cd patchparser && cargo test )
	cargo test

test: cargo_test build_test
	@echo "++ Run tests on test/div"
	test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	test/run-test-for-input-dir test/chj-home

test_opt:
	cargo build --quiet --release
	@echo "++ Run tests on test/div"
	SPLIT_PATCH=target/release/split-patch test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	SPLIT_PATCH=target/release/split-patch test/run-test-for-input-dir test/chj-home

miri_test:
	cargo +nightly miri test --target powerpc-unknown-linux-gnu

miri_run:
	SPLIT_PATCH=test/miri-split-patch test/run-test-for-input-dir test/div

miri: miri_test miri_run
