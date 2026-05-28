# CoreLink Python SDK — development targets
#
# Usage:
#   make install    — install the package in editable mode with dev extras
#   make lint       — run ruff linter
#   make typecheck  — run mypy in strict mode
#   make test       — run pytest with verbose output
#   make ci         — run all checks (install → lint → typecheck → test)
#
# All targets pass if and only if the corresponding tool exits 0.

.DEFAULT_GOAL := ci

PYTHON ?= python3
PIP    ?= pip3

.PHONY: install lint typecheck test ci

install:
	$(PIP) install -e ".[dev]"

lint:
	$(PYTHON) -m ruff check corelink/

typecheck:
	$(PYTHON) -m mypy corelink/

test:
	$(PYTHON) -m pytest -v

ci: install lint typecheck test
