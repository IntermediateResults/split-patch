build:
	cargo build --release

build_test:
	cargo build --quiet

test: build_test
	@echo "++ Run tests on test/div"
	test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	test/run-test-for-input-dir test/chj-home

build_test_opt:
	cargo build --quiet --release

test_opt: build_test_opt
	@echo "++ Run tests on test/div"
	SPLIT_PATCH=target/release/split-patch test/run-test-for-input-dir test/div
	@echo "++ Run tests on test/chj-home"
	SPLIT_PATCH=target/release/split-patch test/run-test-for-input-dir test/chj-home

