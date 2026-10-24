"""Transcripts of runs in the object store (spec 7.4): kept 90 days, then purged."""

import asyncio
from typing import Any, Protocol

import boto3


class Transcripts(Protocol):
    async def put(self, key: str, data: bytes) -> None: ...
    async def delete(self, key: str) -> None: ...


class S3Transcripts:
    def __init__(self, settings: Any) -> None:
        self._bucket = settings.s3_bucket
        self._client = boto3.client(
            "s3",
            endpoint_url=settings.s3_endpoint,
            aws_access_key_id=settings.s3_access_key,
            aws_secret_access_key=settings.s3_secret_key,
            region_name=settings.s3_region,
        )

    async def put(self, key: str, data: bytes) -> None:
        await asyncio.to_thread(
            self._client.put_object, Bucket=self._bucket, Key=key, Body=data, ContentType="application/json"
        )

    async def delete(self, key: str) -> None:
        await asyncio.to_thread(self._client.delete_object, Bucket=self._bucket, Key=key)


class MemoryTranscripts:
    def __init__(self) -> None:
        self.objects: dict[str, bytes] = {}

    async def put(self, key: str, data: bytes) -> None:
        self.objects[key] = data

    async def delete(self, key: str) -> None:
        self.objects.pop(key, None)
