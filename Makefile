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
	_RJEM_MALLOC_CONF="prof:true,prof_prefix:jeprof.out" make test_integration

test_deny_warnings:
	RUSTFLAGS="--deny warnings" make cargo_test

miri_test:
	cargo +nightly miri test --target powerpc-unknown-linux-gnu

miri_run:
	SPLIT_PATCH=test/miri-split-patch test/run-test-for-input-dir test/div

miri: miri_test miri_run
