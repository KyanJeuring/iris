BINARY := iris
PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
ARGS ?=

.PHONY: build release run install update uninstall clean check test fmt lint

build:
	cargo build

release:
	cargo build --release

run:
	cargo run -- $(ARGS)

install: release
	install -Dm755 target/release/$(BINARY) $(BINDIR)/$(BINARY)

update:
	git pull --ff-only
	$(MAKE) install

uninstall:
	rm -f $(BINDIR)/$(BINARY)

clean:
	cargo clean

check:
	cargo check

test:
	cargo test --all-features --locked

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings
