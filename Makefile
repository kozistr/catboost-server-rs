.PHONY: format build-server build-client

format:
	cargo fmt

build-server:
	cargo run --release --bin cb-server

build-client:
	cargo run --release --bin cb-client http://127.0.0.1:50051 32 100000 10
