install:
	poetry install &&\
	\
	poetry show -v>.tmp_poetry &&\
	export pythonPath=`grep -r "virtualenv" .tmp_poetry | sed 's/.*: //1'` &&\
	rm .tmp_poetry &&\
	cat .vscode/settings_template.json | sed "s|\"python.defaultInterpreterPath\": \".*\"|\"python.defaultInterpreterPath\": \"$pythonPath/bin/python\"|1" > .vscode/settings.json
codegen:
	(cd ../../../cli && cargo run -- fmt ../clients/redis/python/memorix_client_redis/example-schema.memorix && cargo run -- codegen ../clients/redis/python/memorix_client_redis/example-schema.memorix) && \
	(cd ../../../cli && cargo run -- fmt ../clients/redis/python/tests/example-schema.memorix && cargo run -- codegen ../clients/redis/python/tests/example-schema.memorix)
format:
	poetry run black .
format-check:
	poetry run black --check .
lint:
	poetry run flake8 .
typecheck:
	poetry run mypy --strict .
test:
	poetry run pytest . -s
