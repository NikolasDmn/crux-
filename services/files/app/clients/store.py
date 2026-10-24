"""Object store behind a protocol: S3 (SeaweedFS) for real, in-memory for tests."""

import asyncio
import hashlib
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from typing import Any, Protocol

import boto3
from botocore.client import Config
from botocore.exceptions import ClientError

CHUNK = 1024 * 1024


class ObjectStore(Protocol):
    async def ensure_bucket(self) -> None: ...
    def presign_put(self, key: str, content_type: str, expires: int) -> str: ...
    def presign_get(self, key: str, file_name: str, expires: int) -> str: ...
    async def size(self, key: str) -> int | None: ...
    def read(self, key: str) -> AsyncIterator[bytes]: ...
    async def delete(self, key: str) -> None: ...
    async def put_bytes(self, key: str, data: bytes, content_type: str) -> None: ...
    async def put_file(self, key: str, fileobj: Any, content_type: str) -> None: ...


async def sha256_and_head(store: ObjectStore, key: str, head_len: int = 8) -> tuple[str, bytes, int]:
    """Streams the object once: its sha256, its first bytes, its length."""
    h = hashlib.sha256()
    head = b""
    total = 0
    async for chunk in store.read(key):
        if len(head) < head_len:
            head += chunk[: head_len - len(head)]
        h.update(chunk)
        total += len(chunk)
    return h.hexdigest(), head, total


class S3Store:
    def __init__(
        self, endpoint: str, public_endpoint: str, access_key: str, secret_key: str, bucket: str, region: str
    ) -> None:
        cfg = Config(signature_version="s3v4", s3={"addressing_style": "path"}, retries={"max_attempts": 3})
        common: dict[str, Any] = {
            "aws_access_key_id": access_key,
            "aws_secret_access_key": secret_key,
            "region_name": region,
            "config": cfg,
        }
        self._s3 = boto3.client("s3", endpoint_url=endpoint, **common)
        # Signs URLs for the browser's origin (the gateway proxies /<bucket>/* to the store).
        self._public = boto3.client("s3", endpoint_url=public_endpoint, **common)
        self.bucket = bucket

    async def ensure_bucket(self) -> None:
        def run() -> None:
            try:
                self._s3.head_bucket(Bucket=self.bucket)
            except ClientError:
                try:
                    self._s3.create_bucket(Bucket=self.bucket)
                except ClientError as e:
                    # Another process created it in between: that's the outcome we wanted.
                    if e.response.get("Error", {}).get("Code") not in (
                        "BucketAlreadyExists",
                        "BucketAlreadyOwnedByYou",
                    ):
                        raise

        await asyncio.to_thread(run)

    def presign_put(self, key: str, content_type: str, expires: int) -> str:
        return str(
            self._public.generate_presigned_url(
                "put_object", Params={"Bucket": self.bucket, "Key": key, "ContentType": content_type}, ExpiresIn=expires
            )
        )

    def presign_get(self, key: str, file_name: str, expires: int) -> str:
        disposition = f'inline; filename="{file_name.replace(chr(34), "")}"'
        return str(
            self._public.generate_presigned_url(
                "get_object",
                Params={"Bucket": self.bucket, "Key": key, "ResponseContentDisposition": disposition},
                ExpiresIn=expires,
            )
        )

    async def size(self, key: str) -> int | None:
        def run() -> int | None:
            try:
                return int(self._s3.head_object(Bucket=self.bucket, Key=key)["ContentLength"])
            except ClientError:
                return None

        return await asyncio.to_thread(run)

    async def read(self, key: str) -> AsyncIterator[bytes]:
        body = await asyncio.to_thread(lambda: self._s3.get_object(Bucket=self.bucket, Key=key)["Body"])
        try:
            while True:
                chunk = await asyncio.to_thread(body.read, CHUNK)
                if not chunk:
                    break
                yield chunk
        finally:
            body.close()

    async def delete(self, key: str) -> None:
        await asyncio.to_thread(lambda: self._s3.delete_object(Bucket=self.bucket, Key=key))

    async def put_bytes(self, key: str, data: bytes, content_type: str) -> None:
        await asyncio.to_thread(
            lambda: self._s3.put_object(Bucket=self.bucket, Key=key, Body=data, ContentType=content_type)
        )

    async def put_file(self, key: str, fileobj: Any, content_type: str) -> None:
        """Streams a (large) local file object up in parts."""
        await asyncio.to_thread(
            lambda: self._s3.upload_fileobj(fileobj, self.bucket, key, ExtraArgs={"ContentType": content_type})
        )


@dataclass
class MemoryStore:
    """Test double. `put` stands in for the browser's PUT to the pre-signed URL."""

    objects: dict[str, bytes] = field(default_factory=dict)

    async def ensure_bucket(self) -> None:
        return None

    def presign_put(self, key: str, content_type: str, expires: int) -> str:
        return f"memory://put/{key}"

    def presign_get(self, key: str, file_name: str, expires: int) -> str:
        return f"memory://get/{key}"

    def put(self, key: str, data: bytes) -> None:
        self.objects[key] = data

    async def size(self, key: str) -> int | None:
        data = self.objects.get(key)
        return None if data is None else len(data)

    async def read(self, key: str) -> AsyncIterator[bytes]:
        data = self.objects[key]
        for i in range(0, len(data), CHUNK):
            yield data[i : i + CHUNK]

    async def delete(self, key: str) -> None:
        self.objects.pop(key, None)

    async def put_bytes(self, key: str, data: bytes, content_type: str) -> None:
        self.objects[key] = data

    async def put_file(self, key: str, fileobj: Any, content_type: str) -> None:
        self.objects[key] = fileobj.read()
