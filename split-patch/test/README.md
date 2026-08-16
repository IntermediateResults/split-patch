# Integration tests for split-patch

## How to run

Normally, run the tests via the `Makefile` in the parent directory.
Go there, then `make test`, `make test_opt` or `make miri`. Setting
`RUST_BACKTRACE=1` should give some location information on the source
of an error.

## Directories

- `div/` contains various hand-picked files with challenges

- `chj-home/` contains the patches of a public repository, as created
  by `git format-patch` using git version 2.47.3

- `errors/` contains patches that test the error response

- `bugs/` contains patches that trigger unresolved bugs; they are not
    run as part of the normal test suite from the `Makefile`. But you can run them via:
  
        test/run-test-for-input-dir test/bugs/
      
    Or just run split-patch on a file directly.
  
    When resolving a bug, move the file that triggered it to `errors/`
    or `div/` (depending on whether split-patch now correctly reports
    an error or splits it correctly, respectively).

## Record a new test file

Add the file to the right directory. Then run 

    test/split-patches-in-dir /opt/chj/bin/split-patch test/"$input_dir" test/"$input_dir"-expected
    git add test/"$input_dir" test/"$input_dir"-expected && git commit

Verify that the Rust version matches via

    test/run-test-for-input-dir test/"$input_dir

then if it doesn't, decide how to resolve (e.g. if decided that it's
actually producing the correct result, move the files from
`test/"$input_dir"-current/"$name_of_patch_file"` to the corresponding
`...-expected` dir).

