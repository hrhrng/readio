.PHONY: dev dev-local dev-minimax dev-web dev-backend dev-extension install clean test test-api test-web test-extension

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
	$(MAKE) test-api
	$(MAKE) test-web
	$(MAKE) test-extension

test-api:
	cd apps/api && uv run --all-groups pytest tests

test-web:
	pnpm --filter @readio/web test

test-extension:
	pnpm --filter @readio/extension test
