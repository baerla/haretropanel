.PHONY: build run test clean js-check

build:
	npm run build
	cargo build --release

run:
	npm run build
	cargo run

test:
	cargo test

clean:
	cargo clean
	rm -rf public/js
	rm -rf node_modules

js-check:
	npm run build
	npm run check
