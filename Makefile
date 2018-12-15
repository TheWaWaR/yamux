fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all -- -D warnings -D clippy::clone_on_ref_ptr -D clippy::enum_glob_use
