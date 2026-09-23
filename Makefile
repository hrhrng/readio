.PHONY: dev dev-local dev-minimax dev-web dev-backend dev-extension install clean test test-core check-wasm wasm-web wasm-cloud test-api test-web test-extension

WASM_BINDGEN_VERSION := 0.2.126
WASM_TARGET := wasm32-unknown-unknown

dev:
	pnpm run dev

dev-local:
	$(MAKE) dev-minimax

dev-minimax:
	pnpm exec concurrently -n web,api -c cyan,green \
		"pnpm --filter @readio/web dev" \
		"cd apps/api && TTS_FALLBACK_ORDER=minimax uv run --all-groups uvicorn app.main:app --reload --port 8000"

dev-web:
	pnpm run dev:web

dev-backend:
	cd apps/api && TTS_FALLBACK_ORDER=minimax uv run --all-groups uvicorn app.main:app --reload --port 8000

dev-extension:
	pnpm run dev:extension

install:
	pnpm install
	uv sync --project apps/api --all-groups

clean:
	pnpm run clean
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
	find . -type f -name '*.pyc' -delete

test:
	$(MAKE) test-core
	$(MAKE) test-api
	$(MAKE) test-web
	$(MAKE) test-extension

test-core:
	# Cargo supplies the Rust sysroot search path while running tests, so the
	# standard library can stay dynamic without affecting release artifacts.
	RUSTFLAGS="$${RUSTFLAGS:+$${RUSTFLAGS} }-C prefer-dynamic" cargo test --workspace

check-wasm:
	RUSTC="$$(rustup which rustc --toolchain stable)" \
	RUSTDOC="$$(rustup which rustdoc --toolchain stable)" \
	cargo check -p readio-wasm --target $(WASM_TARGET)

wasm-web:
	@test "$$(wasm-bindgen --version)" = "wasm-bindgen $(WASM_BINDGEN_VERSION)" || (echo "Expected wasm-bindgen $(WASM_BINDGEN_VERSION)" >&2; exit 1)
	RUSTC="$$(rustup which rustc --toolchain stable)" \
	RUSTDOC="$$(rustup which rustdoc --toolchain stable)" \
	cargo build --release -p readio-wasm --target $(WASM_TARGET)
	wasm-bindgen --target web --out-dir apps/web/wasm --out-name readio_wasm \
		target/$(WASM_TARGET)/release/readio_wasm.wasm

wasm-cloud:
	@test "$$(wasm-bindgen --version)" = "wasm-bindgen $(WASM_BINDGEN_VERSION)" || (echo "Expected wasm-bindgen $(WASM_BINDGEN_VERSION)" >&2; exit 1)
	RUSTC="$$(rustup which rustc --toolchain stable)" \
	RUSTDOC="$$(rustup which rustdoc --toolchain stable)" \
	cargo build --release -p readio-wasm --target $(WASM_TARGET)
	wasm-bindgen --target web --out-dir apps/cloud/wasm --out-name readio_wasm \
		target/$(WASM_TARGET)/release/readio_wasm.wasm

test-api:
	cd apps/api && uv run --all-groups pytest tests

test-web:
	pnpm --filter @readio/web test

test-extension:
	pnpm --filter @readio/extension test
