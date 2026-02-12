.PHONY: dev dev-local dev-minimax dev-web dev-backend dev-extension install clean test test-api test-web test-extension

dev:
	npm run dev

dev-local:
	$(MAKE) dev-minimax

dev-minimax:
	npx concurrently -n web,api -c cyan,green \
		"npm run dev --workspace @readio/web" \
		"cd apps/api && TTS_FALLBACK_ORDER=minimax uv run --all-groups uvicorn app.main:app --reload --port 8000"

dev-web:
	npm run dev:web

dev-backend:
	cd apps/api && TTS_FALLBACK_ORDER=minimax uv run --all-groups uvicorn app.main:app --reload --port 8000

dev-extension:
	npm run dev:extension

install:
	npm install
	uv sync --project apps/api --all-groups

clean:
	npm run clean
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
	find . -type f -name '*.pyc' -delete

test:
	$(MAKE) test-api
	$(MAKE) test-web
	$(MAKE) test-extension

test-api:
	cd apps/api && uv run --all-groups pytest tests

test-web:
	npm run test --workspace @readio/web

test-extension:
	npm run test --workspace @readio/extension
