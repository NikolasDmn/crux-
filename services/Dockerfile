# One image per Python service. Build context: the repo root. ARG SERVICE = files | notify | ai.
FROM python:3.12-slim
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy
COPY --from=ghcr.io/astral-sh/uv:0.4.30 /uv /usr/local/bin/uv
ARG SERVICE
WORKDIR /app
COPY services/pyproject.toml services/uv.lock ./
COPY services/common ./common
COPY services/files/pyproject.toml ./files/pyproject.toml
COPY services/notify/pyproject.toml ./notify/pyproject.toml
COPY services/ai/pyproject.toml ./ai/pyproject.toml
COPY services/${SERVICE} ./${SERVICE}
RUN uv sync --frozen --no-dev --package crux-${SERVICE}
COPY contracts /app/contracts
COPY services/entrypoint.sh /entrypoint.sh
RUN useradd --system --uid 10001 crux && chmod +x /entrypoint.sh
ENV PATH=/app/.venv/bin:$PATH CRUX_CONTRACTS_DIR=/app/contracts
WORKDIR /app/${SERVICE}
USER crux
EXPOSE 8080
ENTRYPOINT ["/entrypoint.sh"]
CMD ["serve"]
