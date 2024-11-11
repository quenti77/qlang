install:
	poetry install
	poetry run pre-commit install
lint:
	poetry run pre-commit run --all-files --hook-stage=manual --show-diff-on-failure
type:
	poetry run mypy .
safety:
	poetry run safety scan --full-report
test:
	poetry run coverage run -m pytest
test-cov:
	poetry run coverage report
test-doc:
	poetry run xdoctest
