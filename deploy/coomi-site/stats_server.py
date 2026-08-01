from __future__ import annotations

import os
import re
from contextlib import contextmanager
from typing import Any, Iterator

import psycopg
from fastapi import FastAPI, Request, Response
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field


EVENT_ID_PATTERN = re.compile(r"^[A-Za-z0-9._:-]{8,128}$")
DATABASE_URL = os.environ["DATABASE_URL"]


class EventPayload(BaseModel):
    eventId: str = Field(min_length=8, max_length=128, pattern=EVENT_ID_PATTERN.pattern)


class StatsStore:
    def __init__(self, database_url: str) -> None:
        self.database_url = database_url
        self.initialize()

    @contextmanager
    def connect(self) -> Iterator[psycopg.Connection[Any]]:
        with psycopg.connect(self.database_url, connect_timeout=5) as connection:
            yield connection

    def initialize(self) -> None:
        with self.connect() as connection, connection.cursor() as cursor:
            cursor.execute(
                """
                CREATE TABLE IF NOT EXISTS counters (
                    id SMALLINT PRIMARY KEY CHECK (id = 1),
                    total_views BIGINT NOT NULL DEFAULT 0 CHECK (total_views >= 0),
                    downloads BIGINT NOT NULL DEFAULT 0 CHECK (downloads >= 0)
                )
                """
            )
            cursor.execute(
                """
                INSERT INTO counters (id, total_views, downloads)
                VALUES (1, 0, 0)
                ON CONFLICT (id) DO NOTHING
                """
            )
            cursor.execute(
                """
                CREATE TABLE IF NOT EXISTS stat_events (
                    event_type VARCHAR(16) NOT NULL CHECK (event_type IN ('view', 'download')),
                    event_id VARCHAR(128) NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (event_type, event_id)
                )
                """
            )
            cursor.execute(
                """
                CREATE INDEX IF NOT EXISTS idx_stat_events_type_created_at
                ON stat_events (event_type, created_at)
                """
            )

    @staticmethod
    def prune(cursor: psycopg.Cursor[Any]) -> None:
        cursor.execute(
            """
            DELETE FROM stat_events
            WHERE (event_type = 'view' AND created_at < CURRENT_TIMESTAMP - INTERVAL '24 hours')
               OR (event_type = 'download' AND created_at < CURRENT_TIMESTAMP - INTERVAL '7 days')
            """
        )

    @staticmethod
    def summary(cursor: psycopg.Cursor[Any]) -> dict[str, int]:
        cursor.execute("SELECT total_views, downloads FROM counters WHERE id = 1")
        total_views, downloads = cursor.fetchone()
        cursor.execute(
            """
            SELECT COUNT(*)
            FROM stat_events
            WHERE event_type = 'view'
              AND created_at >= CURRENT_TIMESTAMP - INTERVAL '24 hours'
            """
        )
        views_24h = cursor.fetchone()[0]
        return {
            "totalViews": int(total_views),
            "views24h": int(views_24h),
            "downloads": int(downloads),
        }

    def get_stats(self) -> dict[str, int]:
        with self.connect() as connection, connection.cursor() as cursor:
            self.prune(cursor)
            return self.summary(cursor)

    def record(self, event_type: str, event_id: str) -> dict[str, int]:
        counter_column = "total_views" if event_type == "view" else "downloads"
        with self.connect() as connection, connection.cursor() as cursor:
            self.prune(cursor)
            cursor.execute(
                """
                INSERT INTO stat_events (event_type, event_id)
                VALUES (%s, %s)
                ON CONFLICT (event_type, event_id) DO NOTHING
                RETURNING 1
                """,
                (event_type, event_id),
            )
            if cursor.fetchone() is not None:
                cursor.execute(f"UPDATE counters SET {counter_column} = {counter_column} + 1 WHERE id = 1")
            return self.summary(cursor)

    def health(self) -> None:
        with self.connect() as connection, connection.cursor() as cursor:
            cursor.execute("SELECT 1")


store = StatsStore(DATABASE_URL)
app = FastAPI(title="Coomi Website Statistics", docs_url=None, redoc_url=None, openapi_url=None)


@app.middleware("http")
async def disable_api_caching(request: Request, call_next: Any) -> Response:
    response = await call_next(request)
    response.headers["Cache-Control"] = "no-store"
    return response


@app.exception_handler(psycopg.Error)
async def handle_database_error(_request: Any, _error: psycopg.Error) -> JSONResponse:
    return JSONResponse(
        status_code=503,
        content={"error": "stats_unavailable"},
        headers={"Cache-Control": "no-store"},
    )


@app.get("/health")
def health() -> dict[str, str]:
    store.health()
    return {"status": "ok"}


@app.get("/api/stats")
def get_stats() -> dict[str, int]:
    return store.get_stats()


@app.post("/api/stats/view")
def record_view(payload: EventPayload) -> dict[str, int]:
    return store.record("view", payload.eventId)


@app.post("/api/stats/download")
def record_download(payload: EventPayload) -> dict[str, int]:
    return store.record("download", payload.eventId)
